use osk_core::gesture::{GestureDetector, GestureEvent, TouchEvent};
use osk_core::layout::KeyPos;

fn k(row: usize, col: usize) -> KeyPos {
    KeyPos { row, col }
}

#[test]
fn tap_commits_on_release() {
    let mut g = GestureDetector::new();
    assert_eq!(
        g.handle(TouchEvent::down(1, 10.0, 10.0, 0), Some(k(1, 0))),
        vec![GestureEvent::Press(k(1, 0))]
    );
    assert_eq!(
        g.handle(TouchEvent::up(1, 12.0, 11.0, 80), Some(k(1, 0))),
        vec![GestureEvent::Tap(k(1, 0))]
    );
    assert_eq!(g.active_touches(), 0);
}

#[test]
fn sliding_off_the_key_cancels() {
    let mut g = GestureDetector::new();
    g.handle(TouchEvent::down(1, 10.0, 10.0, 0), Some(k(1, 0)));
    assert_eq!(
        g.handle(TouchEvent::mv(1, 60.0, 10.0, 40), Some(k(1, 1))),
        vec![GestureEvent::Cancel(k(1, 0))]
    );
    assert_eq!(
        g.handle(TouchEvent::up(1, 60.0, 10.0, 90), Some(k(1, 1))),
        vec![GestureEvent::Cancel(k(1, 0))]
    );
}

#[test]
fn holding_fires_long_press_then_repeats() {
    let mut g = GestureDetector::new();
    g.long_press_ms = 400;
    g.repeat_ms = 50;
    g.handle(TouchEvent::down(1, 10.0, 10.0, 1_000), Some(k(3, 8)));
    assert_eq!(g.tick(1_200), Vec::new());
    assert_eq!(g.tick(1_400), vec![GestureEvent::LongPress(k(3, 8))]);
    assert_eq!(g.tick(1_420), Vec::new(), "repeat waits one interval");
    assert_eq!(g.tick(1_460), vec![GestureEvent::Repeat(k(3, 8))]);
    // Releasing after a long press must not also commit a tap.
    assert_eq!(
        g.handle(TouchEvent::up(1, 10.0, 10.0, 1_500), Some(k(3, 8))),
        vec![GestureEvent::Cancel(k(3, 8))]
    );
}

#[test]
fn two_thumbs_are_tracked_independently() {
    let mut g = GestureDetector::new();
    g.handle(TouchEvent::down(1, 10.0, 10.0, 0), Some(k(1, 0)));
    g.handle(TouchEvent::down(2, 200.0, 10.0, 20), Some(k(1, 9)));
    assert_eq!(g.active_touches(), 2);
    // The second finger never cancels the first.
    assert_eq!(
        g.handle(TouchEvent::up(2, 200.0, 10.0, 60), Some(k(1, 9))),
        vec![GestureEvent::Tap(k(1, 9))]
    );
    assert_eq!(
        g.handle(TouchEvent::up(1, 10.0, 10.0, 90), Some(k(1, 0))),
        vec![GestureEvent::Tap(k(1, 0))]
    );
}

#[test]
fn horizontal_release_becomes_a_swipe() {
    let mut g = GestureDetector::new();
    g.handle(TouchEvent::down(1, 300.0, 300.0, 0), Some(k(4, 4)));
    // Space is 4.5 keys wide, so staying on it while sliding is realistic.
    g.handle(TouchEvent::mv(1, 360.0, 302.0, 50), Some(k(4, 4)));
    assert_eq!(
        g.handle(TouchEvent::up(1, 360.0, 302.0, 90), Some(k(4, 4))),
        vec![GestureEvent::Swipe {
            key: k(4, 4),
            dx: 60.0,
            dy: 2.0
        }]
    );
}

#[test]
fn cancellation_clears_the_touch() {
    let mut g = GestureDetector::new();
    g.handle(TouchEvent::down(1, 10.0, 10.0, 0), Some(k(1, 0)));
    assert_eq!(
        g.handle(TouchEvent::cancel(1, 30), None),
        vec![GestureEvent::Cancel(k(1, 0))]
    );
    assert_eq!(g.active_touches(), 0);
    assert_eq!(g.tick(5_000), Vec::new());
}
