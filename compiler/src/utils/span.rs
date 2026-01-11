use core::ops::Range;

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        assert!(start <= end, "Span start must be <= end");
        Self { start, end }
    }

    pub const fn len(&self) -> usize {
        self.end - self.start
    }

    pub const fn is_empty(&self) -> bool {
        self.start >= self.end
    }

    pub const fn contains(&self, pos: usize) -> bool {
        pos >= self.start && pos < self.end
    }

    pub const fn contains_span(&self, other: Span) -> bool {
        other.start >= self.start && other.end <= self.end
    }

    pub fn to(self, other: Span) -> Self {
        Self::new(
            std::cmp::min(self.start, other.start),
            std::cmp::max(self.end, other.end),
        )
    }
}

/// let span: Span = (0..10).into();
impl From<Range<usize>> for Span {
    fn from(range: Range<usize>) -> Self {
        Self::new(range.start, range.end)
    }
}

///  &str[span.into()]
impl From<Span> for Range<usize> {
    fn from(span: Span) -> Self {
        span.start..span.end
    }
}
