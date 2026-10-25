//! CPU-local successful non-urgent selection budget. The caller retains its
//! Ready owner, supplies actual indexed heads and commits the returned policy.
#[path = "selection/self_test.rs"]
mod self_test;
pub use self_test::run as self_test;
pub(crate) const ROLE_COUNT: usize = 4;
#[derive(Clone, Copy)]
pub(crate) struct Fairness(u8);
pub(crate) enum Order {
    Fifo,
    Rank(u8),
}
pub(crate) struct Pick<T> {
    pub task: Option<T>,
    pub fairness: Fairness,
    pub fifo: bool,
}
impl Fairness {
    pub(crate) const EMPTY: Self = Self(0);
    pub(crate) fn choose<T>(
        self,
        urgent: bool,
        mut head: impl FnMut(Order) -> Option<T>,
    ) -> Pick<T> {
        let fifo = !urgent && self.0 == 7;
        let task = if fifo {
            head(Order::Fifo)
        } else {
            let mut selected = None;
            for rank in 0..ROLE_COUNT as u8 {
                selected = head(Order::Rank(rank));
                if selected.is_some() {
                    break;
                }
            }
            selected
        };
        // Seven completed non-urgent rank picks owe one FIFO pick. Urgency
        // and emptiness can defer that opportunity but never consume it.
        let fairness = if !urgent && task.is_some() {
            if fifo { Self::EMPTY } else { Self(self.0 + 1) }
        } else {
            self
        };
        Pick {
            task,
            fairness,
            fifo,
        }
    }
}
