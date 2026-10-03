use gpui::{AssetSource, Result, SharedString};
use std::borrow::Cow;

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        let strokes = match path {
            "icons/chevron-down.svg" => "<path d='m6 9 6 6 6-6'/>",
            "icons/check.svg" => "<path d='m4 12 5 5 11-11'/>",
            "icons/close.svg" => "<path d='m6 6 12 12M6 18 18 6'/>",
            "icons/copy.svg" => {
                "<rect x='8' y='8' width='12' height='13' rx='2'/><path d='M16 8V3H3v13h5'/>"
            }
            "icons/ellipsis.svg" => {
                "<circle cx='5' cy='12' r='1'/><circle cx='12' cy='12' r='1'/><circle cx='19' cy='12' r='1'/>"
            }
            "toolbar/bold.svg" => "<path d='M7 4h6a4 4 0 0 1 0 8H7zm0 8h7a4 4 0 0 1 0 8H7z'/>",
            "toolbar/italic.svg" => "<path d='M10 4h9M5 20h9M15 4 9 20'/>",
            "toolbar/code.svg" => "<path d='m7 7-5 5 5 5m10-10 5 5-5 5m-3-13-4 20'/>",
            "toolbar/link.svg" => {
                "<path d='m10 14 4-4m-5-1 3-3a5 5 0 0 1 7 7l-3 3m-1-1-3 3a5 5 0 0 1-7-7l3-3'/>"
            }
            "toolbar/image.svg" => {
                "<rect x='3' y='3' width='18' height='18' rx='2'/><circle cx='8' cy='8' r='1'/><path d='m3 17 6-6 4 4 3-3 5 5'/>"
            }
            "toolbar/quote.svg" => "<path d='M4 5h6v8H5v6m9-14h6v8h-5v6'/>",
            "toolbar/list.svg" => {
                "<path d='M9 5h12M9 12h12M9 19h12'/><circle cx='3' cy='5' r='1'/><circle cx='3' cy='12' r='1'/><circle cx='3' cy='19' r='1'/>"
            }
            _ => return Ok(None),
        };
        Ok(Some(Cow::Owned(format!("<svg xmlns='http://www.w3.org/2000/svg' width='24' height='24' viewBox='0 0 24 24' fill='none' stroke='black' stroke-width='1.7' stroke-linecap='round' stroke-linejoin='round'>{strokes}</svg>").into_bytes())))
    }

    fn list(&self, _: &str) -> Result<Vec<SharedString>> {
        Ok(vec![])
    }
}
