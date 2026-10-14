//! Bounded CPU caches shared by all editor windows for this application session.
use crate::diagrams::{Diagram, DiagramKey, RasterSize};
use gpui::{App, Global, Image, ImageId, RenderImage, Window};
use std::{
    collections::HashMap,
    hash::Hash,
    sync::{Arc, Mutex, Weak},
};

const SVG_BUDGET: usize = 16 * 1024 * 1024;
const PIXEL_BUDGET: usize = 256 * 1024 * 1024;
const ENTRY_LIMIT: usize = 128;

type RequestMap<K> = Mutex<HashMap<K, Weak<RequestGate<K>>>>;

struct RequestLocks<K: Eq + Hash + Clone>(Arc<RequestMap<K>>);

impl<K: Eq + Hash + Clone> Default for RequestLocks<K> {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(HashMap::new())))
    }
}

pub(crate) struct RequestGate<K: Eq + Hash + Clone> {
    mutex: Arc<async_lock::Mutex<()>>,
    key: K,
    owner: Weak<RequestMap<K>>,
}

pub(crate) struct RequestPermit<K: Eq + Hash + Clone> {
    // Release the mutex before retiring the last request registration.
    _guard: async_lock::MutexGuardArc<()>,
    _request: Arc<RequestGate<K>>,
}

impl<K: Eq + Hash + Clone> RequestLocks<K> {
    fn get(&self, key: &K) -> Arc<RequestGate<K>> {
        let mut requests = self.0.lock().expect("diagram request locks");
        if let Some(request) = requests.get(key).and_then(Weak::upgrade) {
            return request;
        }
        let request = Arc::new(RequestGate {
            mutex: Arc::new(async_lock::Mutex::new(())),
            key: key.clone(),
            owner: Arc::downgrade(&self.0),
        });
        requests.insert(key.clone(), Arc::downgrade(&request));
        request
    }
}

impl<K: Eq + Hash + Clone> RequestGate<K> {
    pub async fn acquire(self: Arc<Self>) -> RequestPermit<K> {
        let guard = self.mutex.lock_arc().await;
        RequestPermit {
            _guard: guard,
            _request: self,
        }
    }

    #[cfg(test)]
    pub fn try_acquire(self: Arc<Self>) -> RequestPermit<K> {
        let guard = self.mutex.try_lock_arc().expect("request is idle");
        RequestPermit {
            _guard: guard,
            _request: self,
        }
    }
}

impl<K: Eq + Hash + Clone> Drop for RequestGate<K> {
    fn drop(&mut self) {
        if let Some(owner) = self.owner.upgrade() {
            let mut requests = owner.lock().expect("diagram request locks");
            if requests
                .get(&self.key)
                .is_some_and(|request| request.strong_count() == 0)
            {
                requests.remove(&self.key);
            }
        }
    }
}

struct Entry<V> {
    value: V,
    bytes: usize,
    used: u64,
}

struct Lru<K, V> {
    entries: HashMap<K, Entry<V>>,
    budget: usize,
    limit: usize,
    bytes: usize,
    clock: u64,
}

impl<K: Eq + Hash + Clone, V: Clone> Lru<K, V> {
    fn new(budget: usize, limit: usize) -> Self {
        Self {
            entries: HashMap::new(),
            budget,
            limit,
            bytes: 0,
            clock: 0,
        }
    }

    fn get(&mut self, key: &K) -> Option<V> {
        let entry = self.entries.get_mut(key)?;
        self.clock += 1;
        entry.used = self.clock;
        Some(entry.value.clone())
    }

    fn insert(&mut self, key: K, value: V, bytes: usize) -> Vec<V> {
        let mut evicted = Vec::new();
        if let Some(old) = self.entries.remove(&key) {
            self.bytes -= old.bytes;
            evicted.push(old.value);
        }
        if bytes <= self.budget && self.limit > 0 {
            self.clock += 1;
            self.entries.insert(
                key,
                Entry {
                    value,
                    bytes,
                    used: self.clock,
                },
            );
            self.bytes += bytes;
        }
        while self.bytes > self.budget || self.entries.len() > self.limit {
            let key = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.used)
                .unwrap()
                .0
                .clone();
            let entry = self.entries.remove(&key).unwrap();
            self.bytes -= entry.bytes;
            evicted.push(entry.value);
        }
        evicted
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) struct RasterKey {
    // Full SVG equality prevents hash collisions from substituting another graph.
    svg: Arc<Image>,
    size: RasterSize,
}

impl RasterKey {
    pub fn new(diagram: &Diagram, size: RasterSize) -> Self {
        Self {
            svg: diagram.image.clone(),
            size,
        }
    }
}

pub(crate) struct DiagramCache {
    layouts: Lru<DiagramKey, Result<Diagram, String>>,
    pixels: Lru<RasterKey, Arc<RenderImage>>,
    // Atlas removal affects every window. Only the last live view can evict
    // shared GPU textures; cached idle pixels do not keep textures alive.
    users: HashMap<ImageId, usize>,
    layout_requests: RequestLocks<DiagramKey>,
    raster_requests: RequestLocks<RasterKey>,
}

impl Global for DiagramCache {}

impl Default for DiagramCache {
    fn default() -> Self {
        Self {
            layouts: Lru::new(SVG_BUDGET, ENTRY_LIMIT),
            pixels: Lru::new(PIXEL_BUDGET, ENTRY_LIMIT),
            users: HashMap::new(),
            layout_requests: RequestLocks::default(),
            raster_requests: RequestLocks::default(),
        }
    }
}

pub(crate) fn layout(cx: &mut App, key: &DiagramKey) -> Option<Result<Diagram, String>> {
    cx.default_global::<DiagramCache>().layouts.get(key)
}

pub(crate) fn request_layout(cx: &mut App, key: &DiagramKey) -> Arc<RequestGate<DiagramKey>> {
    cx.default_global::<DiagramCache>().layout_requests.get(key)
}

pub(crate) fn request_raster(cx: &mut App, key: &RasterKey) -> Arc<RequestGate<RasterKey>> {
    cx.default_global::<DiagramCache>().raster_requests.get(key)
}

pub(crate) fn store_layout(cx: &mut App, key: &DiagramKey, result: &Result<Diagram, String>) {
    let bytes = key.source.len()
        + match result {
            Ok(diagram) => diagram.image.bytes().len(),
            Err(error) => error.len(),
        };
    cx.default_global::<DiagramCache>()
        .layouts
        .insert(key.clone(), result.clone(), bytes);
}

pub(crate) fn pixels(cx: &mut App, key: &RasterKey) -> Option<Arc<RenderImage>> {
    cx.default_global::<DiagramCache>().pixels.get(key)
}

pub(crate) fn store_pixels(cx: &mut App, key: RasterKey, image: Arc<RenderImage>) {
    let bytes = key.size.bytes() + key.svg.bytes().len();
    let evicted = cx
        .default_global::<DiagramCache>()
        .pixels
        .insert(key, image, bytes);
    for image in evicted {
        if !cx.global::<DiagramCache>().users.contains_key(&image.id) {
            drop_idle_texture(cx, image);
        }
    }
}

pub(crate) fn pin(cx: &mut App, image: &Arc<RenderImage>) {
    *cx.default_global::<DiagramCache>()
        .users
        .entry(image.id)
        .or_default() += 1;
}

pub(crate) fn release(cx: &mut App, image: Arc<RenderImage>, window: Option<&mut Window>) {
    let users = &mut cx.default_global::<DiagramCache>().users;
    if let Some(count) = users.get_mut(&image.id) {
        *count -= 1;
        if *count != 0 {
            return;
        }
        users.remove(&image.id);
    }
    if let Some(window) = window {
        cx.drop_image(image, Some(window));
    } else {
        drop_idle_texture(cx, image);
    }
}

fn drop_idle_texture(cx: &mut App, image: Arc<RenderImage>) {
    // During rendering the current window is temporarily outside App.windows.
    // Defer atlas cleanup until it is returned, and account for a new cache hit
    // that may have attached another view before the deferred effect runs.
    cx.defer(move |cx| {
        if cx
            .try_global::<DiagramCache>()
            .is_none_or(|cache| !cache.users.contains_key(&image.id))
        {
            cx.drop_image(image, None);
        }
    });
}

#[cfg(test)]
pub(crate) fn clear_pixels(cx: &mut App) {
    let entries = std::mem::take(&mut cx.default_global::<DiagramCache>().pixels.entries);
    cx.default_global::<DiagramCache>().pixels.bytes = 0;
    for entry in entries.into_values() {
        if !cx
            .global::<DiagramCache>()
            .users
            .contains_key(&entry.value.id)
        {
            drop_idle_texture(cx, entry.value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagram_image::DiagramImage;
    use gpui::{AppContext, TestAppContext};

    fn diagram() -> Diagram {
        crate::diagrams::render(&DiagramKey {
            source: "flowchart LR\nA --> B".into(),
            dark: false,
        })
        .unwrap()
    }

    #[gpui::test]
    fn shared_pixels_survive_one_view_closing_and_eviction_releases_the_last_view(
        cx: &mut TestAppContext,
    ) {
        let diagram = diagram();
        let key = RasterKey::new(
            &diagram,
            RasterSize {
                width: 100,
                height: 40,
            },
        );
        let first = cx.new(|cx| DiagramImage::new(diagram.clone(), cx));
        first.update(cx, |view, cx| view.set_display_size(100.0, 40.0, 1.0, cx));
        cx.run_until_parked();
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(101));
        cx.run_until_parked();
        let (id, weak) = cx.update(|cx| {
            let pixels = pixels(cx, &key).unwrap();
            (pixels.id, Arc::downgrade(&pixels))
        });
        let second = cx.new(|cx| DiagramImage::new(diagram, cx));
        second.update(cx, |view, cx| view.set_display_size(100.0, 40.0, 1.0, cx));
        assert_eq!(cx.read(|cx| cx.global::<DiagramCache>().users[&id]), 2);
        drop(first);
        cx.update(|_| ());
        assert_eq!(cx.read(|cx| cx.global::<DiagramCache>().users[&id]), 1);
        cx.update(clear_pixels);
        assert!(
            weak.upgrade().is_some(),
            "eviction cannot unload an active view"
        );
        assert!(cx.read(|cx| second.read(cx).decoded_size().is_some()));
        drop(second);
        cx.update(|_| ());
        assert!(weak.upgrade().is_none());
        assert!(!cx.read(|cx| cx.global::<DiagramCache>().users.contains_key(&id)));
    }

    #[gpui::test]
    fn pixel_entry_eviction_does_not_unload_live_views(cx: &mut TestAppContext) {
        cx.update(|cx| {
            cx.set_global(DiagramCache {
                pixels: Lru::new(100000, 1),
                ..Default::default()
            });
        });
        let diagram = diagram();
        let first_key = RasterKey::new(
            &diagram,
            RasterSize {
                width: 100,
                height: 40,
            },
        );
        let first = cx.new(|cx| DiagramImage::new(diagram.clone(), cx));
        first.update(cx, |view, cx| view.set_display_size(100.0, 40.0, 1.0, cx));
        cx.run_until_parked();
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(101));
        cx.run_until_parked();
        let weak = cx.update(|cx| Arc::downgrade(&pixels(cx, &first_key).unwrap()));
        let second = cx.new(|cx| DiagramImage::new(diagram, cx));
        second.update(cx, |view, cx| view.set_display_size(110.0, 40.0, 1.0, cx));
        cx.run_until_parked();
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(101));
        cx.run_until_parked();
        assert!(cx.update(|cx| pixels(cx, &first_key).is_none()));
        assert!(weak.upgrade().is_some());
        drop(first);
        cx.update(|_| ());
        assert!(
            weak.upgrade().is_none(),
            "evicted pixels must release when their last view closes"
        );
    }

    #[test]
    fn cache_limits_bytes_and_entries_and_keeps_recently_used_results() {
        let mut cache = Lru::new(10, 2);
        cache.insert("first", "one", 4);
        cache.insert("second", "two", 4);
        assert_eq!(cache.get(&"first"), Some("one"));
        assert_eq!(cache.insert("third", "three", 4), vec!["two"]);
        assert_eq!(cache.get(&"second"), None);
        assert_eq!(cache.get(&"first"), Some("one"));
        assert_eq!(cache.bytes, 8);
        assert_eq!(cache.insert("large", "large", 10).len(), 2);
        assert_eq!(cache.bytes, 10);
        cache.insert("oversized", "too big", 11);
        assert_eq!(cache.get(&"oversized"), None);
        assert_eq!(cache.get(&"large"), Some("large"));
        let mut cache = Lru::new(100, 1);
        cache.insert(1, "a", 1);
        cache.insert(2, "b", 1);
        assert_eq!(cache.get(&1), None);
        assert_eq!(cache.get(&2), Some("b"));
    }
}
