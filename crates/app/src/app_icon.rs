use objc2::{AnyThread, MainThreadMarker};
use objc2_app_kit::{NSApplication, NSImage};
use objc2_foundation::NSData;

// cargo run launches a bare executable, without the .app bundle's Info.plist.
// Embed the same multi-resolution icon so the Dock uses it for either launch path.
const ICON: &[u8] = include_bytes!("../../../assets/icons/tiny-md.icns");

pub fn install() {
    let mtm = MainThreadMarker::new().expect("app icon must be installed on the main thread");
    let data = NSData::with_bytes(ICON);
    let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) else {
        eprintln!("failed to decode the embedded Tiny MD app icon");
        return;
    };
    let application = NSApplication::sharedApplication(mtm);
    // SAFETY: This runs on the main thread and supplies a valid, non-null NSImage.
    unsafe { application.setApplicationIconImage(Some(&image)) };
}
