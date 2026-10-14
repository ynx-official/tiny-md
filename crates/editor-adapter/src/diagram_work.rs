//! Share diagram capacity across document and viewer windows in this app.
use async_lock::Semaphore;
use gpui::{App, Global};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

pub(crate) const MAX_CONCURRENT: usize = 5;

pub(crate) struct DiagramWork {
    capacity: Arc<Semaphore>,
    layout_order: WorkOrder,
    raster_order: WorkOrder,
}

impl Global for DiagramWork {}

impl Default for DiagramWork {
    fn default() -> Self {
        // Both stages and all windows share one ceiling, rather than allowing
        // five layouts plus another five decoders to run simultaneously.
        Self {
            capacity: Arc::new(Semaphore::new(MAX_CONCURRENT)),
            layout_order: WorkOrder::default(),
            raster_order: WorkOrder::default(),
        }
    }
}

pub(crate) fn layout(cx: &mut App) -> Arc<Semaphore> {
    cx.default_global::<DiagramWork>().capacity.clone()
}

pub(crate) fn raster(cx: &mut App) -> Arc<Semaphore> {
    cx.default_global::<DiagramWork>().capacity.clone()
}

// Reserve synchronously, before debounce or executor polling can reorder jobs.
// Admit a window of five jobs, but publish results in source order. Keeping at
// most five unpublished results per lane bounds memory behind a slow first job.
// Canceled tickets leave the queue even if their future was never polled.
#[derive(Clone, Default)]
struct WorkOrder(Arc<Mutex<VecDeque<Arc<WorkSlot>>>>);

struct WorkSlot {
    ready: Arc<Semaphore>,
    publish: Arc<Semaphore>,
}

pub(crate) struct WorkTicket {
    order: WorkOrder,
    slot: Arc<WorkSlot>,
}

impl WorkOrder {
    fn reserve(&self) -> WorkTicket {
        let mut queue = self.0.lock().expect("diagram work queue");
        let slot = Arc::new(WorkSlot {
            ready: Arc::new(Semaphore::new(usize::from(queue.len() < MAX_CONCURRENT))),
            publish: Arc::new(Semaphore::new(usize::from(queue.is_empty()))),
        });
        queue.push_back(slot.clone());
        WorkTicket {
            order: self.clone(),
            slot,
        }
    }
}

impl WorkTicket {
    pub async fn wait(self) -> Self {
        drop(self.slot.ready.acquire_arc().await);
        self
    }

    pub async fn publish(&self) {
        drop(self.slot.publish.acquire_arc().await);
    }
}

impl Drop for WorkTicket {
    fn drop(&mut self) {
        let (next_work, next_publish) = {
            let mut queue = self.order.0.lock().expect("diagram work queue");
            let index = queue
                .iter()
                .position(|slot| Arc::ptr_eq(slot, &self.slot))
                .unwrap();
            queue.remove(index);
            (
                (index < MAX_CONCURRENT)
                    .then(|| queue.get(MAX_CONCURRENT - 1).cloned())
                    .flatten(),
                (index == 0).then(|| queue.front().cloned()).flatten(),
            )
        };
        if let Some(next) = next_work {
            next.ready.add_permits(1);
        }
        if let Some(next) = next_publish {
            next.publish.add_permits(1);
        }
    }
}

pub(crate) fn reserve_layout(cx: &mut App) -> WorkTicket {
    cx.default_global::<DiagramWork>().layout_order.reserve()
}

pub(crate) fn reserve_raster(cx: &mut App) -> WorkTicket {
    cx.default_global::<DiagramWork>().raster_order.reserve()
}

#[cfg(test)]
pub(crate) fn occupy(capacity: &Arc<Semaphore>) -> Vec<async_lock::SemaphoreGuardArc> {
    let mut permits = Vec::new();
    while let Some(permit) = capacity.try_acquire_arc() {
        permits.push(permit);
    }
    permits
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;
    use std::{
        sync::atomic::{AtomicUsize, Ordering},
        time::Duration,
    };

    #[gpui::test]
    fn shared_render_capacity_runs_five_jobs_and_never_a_sixth(cx: &mut TestAppContext) {
        let active = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        let entered = Arc::new(AtomicUsize::new(0));
        let executor = cx.executor();
        let mut tasks = Vec::new();
        for index in 0..16 {
            let (capacity, ticket) = cx.update(|cx| {
                if index % 2 == 0 {
                    (layout(cx), reserve_layout(cx))
                } else {
                    (raster(cx), reserve_raster(cx))
                }
            });
            let (active, maximum, entered) = (active.clone(), maximum.clone(), entered.clone());
            let executor = executor.clone();
            tasks.push(executor.clone().spawn(async move {
                let _ticket = ticket.wait().await;
                let permit = capacity.acquire_arc().await;
                let count = active.fetch_add(1, Ordering::SeqCst) + 1;
                maximum.fetch_max(count, Ordering::SeqCst);
                entered.fetch_add(1, Ordering::SeqCst);
                executor.timer(Duration::from_secs(1)).await;
                active.fetch_sub(1, Ordering::SeqCst);
                drop(permit);
            }));
        }
        cx.run_until_parked();
        assert_eq!(
            active.load(Ordering::SeqCst),
            5,
            "five diagram jobs must run at once"
        );
        assert_eq!(
            entered.load(Ordering::SeqCst),
            5,
            "the sixth job must wait for capacity"
        );
        for _ in 0..16 {
            cx.executor().advance_clock(Duration::from_secs(1));
            cx.run_until_parked();
        }
        assert_eq!(entered.load(Ordering::SeqCst), 16);
        assert_eq!(
            maximum.load(Ordering::SeqCst),
            5,
            "layout and raster jobs must share the same limit"
        );
        assert_eq!(active.load(Ordering::SeqCst), 0);
        let capacity = cx.update(layout);
        assert_eq!(occupy(&capacity).len(), 5, "all slots must be returned");
        drop(tasks);
    }
}
