//! Ordered focus stack for toplevels.
//!
//! Pure, smithay-free so the ordering policy is unit-testable: last element
//! is the input-focus target, destroying the top window falls back to the
//! previous one instead of "whatever `space.elements()` returns".

/// Ordered window stack; the last element is topmost (has focus).
#[derive(Debug)]
pub struct FocusStack<T> {
    order: Vec<T>,
}

impl<T> Default for FocusStack<T> {
    fn default() -> Self {
        Self { order: Vec::new() }
    }
}

impl<T: PartialEq> FocusStack<T> {
    /// Raise `item` to the top (input focus). Re-raising an existing item
    /// moves it without duplicating it.
    pub fn raise(&mut self, item: T) {
        self.order.retain(|existing| existing != &item);
        self.order.push(item);
    }

    /// Drop `item` wherever it sits in the stack (unmap/destroy).
    pub fn remove(&mut self, item: &T) {
        self.order.retain(|existing| existing != item);
    }

    /// Current focus target: the topmost surviving window.
    pub fn top(&self) -> Option<&T> {
        self.order.last()
    }

    pub fn len(&self) -> usize {
        self.order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::FocusStack;

    #[test]
    fn raise_appends_and_reorders() {
        let mut stack = FocusStack::default();
        stack.raise(1);
        stack.raise(2);
        stack.raise(3);
        assert_eq!(stack.top(), Some(&3));
        stack.raise(1);
        assert_eq!(stack.top(), Some(&1));
        assert_eq!(stack.len(), 3);
    }

    #[test]
    fn removing_top_falls_back_to_previous() {
        let mut stack = FocusStack::default();
        for w in [10, 20, 30] {
            stack.raise(w);
        }
        stack.remove(&30);
        assert_eq!(stack.top(), Some(&20));
        stack.remove(&20);
        assert_eq!(stack.top(), Some(&10));
        stack.remove(&10);
        assert_eq!(stack.top(), None);
        assert!(stack.is_empty());
    }

    #[test]
    fn removing_middle_keeps_top() {
        let mut stack = FocusStack::default();
        for w in [1, 2, 3] {
            stack.raise(w);
        }
        stack.remove(&2);
        assert_eq!(stack.top(), Some(&3));
        assert_eq!(stack.len(), 2);
    }

    #[test]
    fn re_raise_after_remove_is_clean() {
        let mut stack = FocusStack::default();
        stack.raise(7);
        stack.raise(8);
        stack.remove(&8);
        stack.raise(8);
        assert_eq!(stack.top(), Some(&8));
        assert_eq!(stack.len(), 2);
    }
}
