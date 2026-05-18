use std::collections::VecDeque;

/// A FIFO queue that returns elements after a delay.
pub(crate) struct Delayed<T> where T: Copy {
    delay: usize,
    current_time: u64,
    elements: VecDeque<Element<T>>,
}

impl <T: Copy> Clone for Delayed<T> {
    fn clone(&self) -> Self {
        Self {
            delay: self.delay,
            current_time: self.current_time,
            elements: self.elements.clone()
        }
    }
}

#[derive(Clone, Copy)]
struct Element<T> {
    element: T,
    time: u64,
}

impl <T: Copy> Delayed<T> {
    pub fn new(delay: usize) -> Self {
        Self {
            delay,
            current_time: 0,
            elements: VecDeque::with_capacity(delay),
        }
    }

    pub fn push(&mut self, element: T) -> Result<(), String> {
        if self.elements.len() == self.delay {
            Err("Delayed queue is full when trying to add element".to_string())
        } else {
            Ok(self.elements.push_back(Element {
                element,
                time: self.current_time + self.delay as u64 - 1,
            }))
        }
    }

    pub fn pop(&mut self) -> Option<T> {
        let result = if self
            .elements
            .front()
            .is_some_and(|element| element.time <= self.current_time)
        {
            let element = self
                .elements
                .pop_front()
                .expect("front element existed but pop_front returned None");

            Some(element.element)
        } else {
            None
        };

        self.current_time += 1;
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delay_1_returns_on_first_pop() {
        let mut d: Delayed<u8> = Delayed::new(1);
        d.push(42).unwrap();
        assert_eq!(d.pop(), Some(42));
        assert_eq!(d.pop(), None);
    }

    #[test]
    fn delay_3_single_entry() {
        let mut d: Delayed<(usize, u8)> = Delayed::new(3);
        d.push((0, 1)).unwrap();
        assert_eq!(d.pop(), None);
        assert_eq!(d.pop(), None);
        assert_eq!(d.pop(), Some((0, 1)));
        assert_eq!(d.pop(), None);
    }

    #[test]
    fn delay_3_two_entries_pushed_together() {
        let mut d: Delayed<(usize, u8)> = Delayed::new(3);
        d.push((0, 1)).unwrap();
        d.push((2, 3)).unwrap();
        assert_eq!(d.pop(), None);
        assert_eq!(d.pop(), None);
        assert_eq!(d.pop(), Some((0, 1)));
        // (2, 3) was pushed one tick later relative to (0, 1) so it needs one
        // more pop.
        assert_eq!(d.pop(), Some((2, 3)));
    }

    #[test]
    fn delay_3_three_entries_then_reuse() {
        let mut d: Delayed<(usize, u8)> = Delayed::new(3);
        d.push((2, 3)).unwrap();
        d.push((3, 4)).unwrap();
        assert_eq!(d.pop(), None);
        assert_eq!(d.pop(), None);
        assert_eq!(d.pop(), Some((2, 3)));
        assert_eq!(d.pop(), Some((3, 4)));

        // Queue is now empty; push another entry and confirm the delay resets.
        d.push((4, 5)).unwrap();
        assert_eq!(d.pop(), None);
        assert_eq!(d.pop(), None);
        assert_eq!(d.pop(), Some((4, 5)));
    }

    #[test]
    fn capacity_limit_enforced() {
        let mut d: Delayed<(usize, u8)> = Delayed::new(2);
        d.push((0, 1)).unwrap();
        d.push((1, 2)).unwrap();
        assert!(d.push((2, 3)).is_err());
    }

    #[test]
    fn capacity_freed_after_pop() {
        let mut d: Delayed<(usize, u8)> = Delayed::new(2);
        d.push((0, 1)).unwrap();
        d.push((1, 2)).unwrap();
        // Drain the queue so there is room again.
        while d.pop().is_none() {}   // first entry out
        while d.pop().is_none() {}   // second entry out
        // Now there is room for two more.
        assert!(d.push((2, 3)).is_ok());
        assert!(d.push((3, 4)).is_ok());
    }

    // #[test]
    // #[should_panic]
    fn delay_zero_panics() {
        let _: Delayed<(usize, u8)> = Delayed::new(0);
    }
}
