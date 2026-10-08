use std::time::{Duration, Instant};

pub(crate) struct CaretBlink {
    activity: Instant,
    pub focused: bool,
    pub visible: bool,
}

impl Default for CaretBlink {
    fn default() -> Self {
        Self {
            activity: Instant::now(),
            focused: false,
            visible: true,
        }
    }
}

impl CaretBlink {
    pub fn reset(&mut self) {
        self.activity = Instant::now();
        self.visible = true;
    }

    pub fn tick(&mut self, composing: bool) -> bool {
        let next = visible_at(self.activity.elapsed(), composing);
        let changed = self.visible != next;
        self.visible = next;
        self.focused && changed
    }
}

fn visible_at(elapsed: Duration, composing: bool) -> bool {
    composing || (elapsed.as_millis() / 500).is_multiple_of(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caret_blinks_every_half_second_and_stays_visible_during_composition() {
        for (ms, visible) in [
            (0, true),
            (499, true),
            (500, false),
            (999, false),
            (1000, true),
        ] {
            assert_eq!(visible_at(Duration::from_millis(ms), false), visible);
            assert!(visible_at(Duration::from_millis(ms), true));
        }
    }

    #[test]
    fn activity_resets_the_caret_and_unfocused_ticks_do_not_request_repaints() {
        let mut caret = CaretBlink {
            activity: Instant::now() - Duration::from_millis(600),
            ..Default::default()
        };
        assert!(!caret.tick(false));
        assert!(!caret.visible);
        caret.reset();
        assert!(caret.visible);
        caret.focused = true;
        caret.activity = Instant::now() - Duration::from_millis(600);
        assert!(caret.tick(false));
        assert!(!caret.visible);
        assert!(caret.tick(true));
        assert!(caret.visible);
    }
}
