//! Share diagram capacity across document and viewer windows in this app.
use async_lock::Semaphore;
use gpui::{App, Global};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

pub(crate) struct DiagramWork {
    pub layout: Arc<Semaphore>,
    pub raster: Arc<Semaphore>,
    layout_order: WorkOrder,
    raster_order: WorkOrder,
}

impl Global for DiagramWork {}

impl Default for DiagramWork {
    fn default() -> Self {
        // Separate lanes let the first previews decode while remaining SVGs
        // are prepared. A document with many diagrams must not fill the CPU pool.
        Self {
            layout: Arc::new(Semaphore::new(1)),
            raster: Arc::new(Semaphore::new(1)),
            layout_order: WorkOrder::default(),
            raster_order: WorkOrder::default(),
        }
    }
}

pub(crate) fn layout(cx: &mut App) -> Arc<Semaphore> {
    cx.default_global::<DiagramWork>().layout.clone()
}

pub(crate) fn raster(cx: &mut App) -> Arc<Semaphore> {
    cx.default_global::<DiagramWork>().raster.clone()
}

// Reserve synchronously, before debounce or executor polling can reorder jobs.
// A ticket stays alive through computation and publication; canceled tickets
// leave the queue, including when their future has never been polled.
#[derive(Clone, Default)]
struct WorkOrder(Arc<Mutex<VecDeque<Arc<Semaphore>>>>);

pub(crate) struct WorkTicket {
    order: WorkOrder,
    ready: Arc<Semaphore>,
}

impl WorkOrder {
    fn reserve(&self) -> WorkTicket {
        let mut queue = self.0.lock().expect("diagram work queue");
        let ready = Arc::new(Semaphore::new(usize::from(queue.is_empty())));
        queue.push_back(ready.clone());
        WorkTicket {
            order: self.clone(),
            ready,
        }
    }
}

impl WorkTicket {
    pub async fn wait(self) -> Self {
        drop(self.ready.acquire_arc().await);
        self
    }
}

impl Drop for WorkTicket {
    fn drop(&mut self) {
        let next = {
            let mut queue = self.order.0.lock().expect("diagram work queue");
            let index = queue
                .iter()
                .position(|ready| Arc::ptr_eq(ready, &self.ready))
                .unwrap();
            queue.remove(index);
            (index == 0).then(|| queue.front().cloned()).flatten()
        };
        if let Some(next) = next {
            next.add_permits(1);
        }
    }
}

pub(crate) fn reserve_layout(cx: &mut App) -> WorkTicket {
    cx.default_global::<DiagramWork>().layout_order.reserve()
}

pub(crate) fn reserve_raster(cx: &mut App) -> WorkTicket {
    cx.default_global::<DiagramWork>().raster_order.reserve()
}
