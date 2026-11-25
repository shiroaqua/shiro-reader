pub mod locator;

pub use locator::{
    BookLocator, BookNavigationError, EpubHref, NormalizedOffset, PdfDest, Progression,
    TocNodeFlags,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinearUnitSpec {
    pub is_linear: bool,
    pub weight: u64,
    pub display_label: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TocEntrySpec {
    pub title: String,
    pub locator: Option<BookLocator>,
    pub unit_offset: NormalizedOffset,
    pub flags: TocNodeFlags,
    pub children: Vec<TocEntrySpec>,
}

/// Physical reading unit forming a continuous monotonic progression axis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinearUnit {
    index: u32,
    is_linear: bool,
    weight: u64,
    cumulative_weight: u64,
    display_label: Option<String>,
}

impl LinearUnit {
    pub const fn index(&self) -> u32 {
        self.index
    }

    pub const fn is_linear(&self) -> bool {
        self.is_linear
    }

    pub const fn weight(&self) -> u64 {
        self.weight
    }

    pub const fn cumulative_weight(&self) -> u64 {
        self.cumulative_weight
    }

    pub fn display_label(&self) -> Option<&str> {
        self.display_label.as_deref()
    }
}

/// Logical TOC node stored in a pre-order flattened array (array index is node ID).
#[derive(Debug, Clone, PartialEq)]
pub struct TocNode {
    parent: Option<u32>,
    depth: u16,
    subtree_end: u32,
    title: String,
    locator: Option<BookLocator>,
    unit_index: Option<u32>,
    unit_offset: NormalizedOffset,
    start_progression: Option<Progression>,
    flags: TocNodeFlags,
}

impl TocNode {
    pub const fn parent(&self) -> Option<u32> {
        self.parent
    }

    pub const fn depth(&self) -> u16 {
        self.depth
    }

    pub const fn subtree_end(&self) -> u32 {
        self.subtree_end
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub const fn locator(&self) -> Option<&BookLocator> {
        self.locator.as_ref()
    }

    pub const fn unit_index(&self) -> Option<u32> {
        self.unit_index
    }

    pub const fn unit_offset(&self) -> NormalizedOffset {
        self.unit_offset
    }

    pub const fn start_progression(&self) -> Option<Progression> {
        self.start_progression
    }

    pub const fn flags(&self) -> TocNodeFlags {
        self.flags
    }
}

/// Unified navigation tree and progression index for a book.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BookNavigation {
    toc: Vec<TocNode>,
    linear_units: Vec<LinearUnit>,
    total_weight: u64,
}

impl BookNavigation {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn build(
        toc_roots: Vec<TocEntrySpec>,
        unit_specs: Vec<LinearUnitSpec>,
    ) -> Result<Self, BookNavigationError> {
        u32::try_from(unit_specs.len()).map_err(|_| BookNavigationError::TooManyLinearUnits)?;

        let mut linear_units = Vec::with_capacity(unit_specs.len());
        let mut running_weight = 0u64;
        for (i, spec) in unit_specs.into_iter().enumerate() {
            let display_label = spec
                .display_label
                .map(|label| label.trim().to_owned())
                .filter(|label| !label.is_empty());
            let cumulative_weight = running_weight;
            if spec.is_linear {
                running_weight = running_weight.saturating_add(spec.weight);
            }
            linear_units.push(LinearUnit {
                index: i as u32,
                is_linear: spec.is_linear,
                weight: spec.weight,
                cumulative_weight,
                display_label,
            });
        }
        let total_weight = running_weight;

        enum Frame {
            Enter {
                spec: TocEntrySpec,
                parent: Option<u32>,
                depth: u16,
            },
            Exit {
                node_index: usize,
            },
        }

        let mut toc = Vec::new();
        let mut stack: Vec<Frame> = toc_roots
            .into_iter()
            .rev()
            .map(|spec| Frame::Enter {
                spec,
                parent: None,
                depth: 0,
            })
            .collect();

        while let Some(frame) = stack.pop() {
            match frame {
                Frame::Enter {
                    spec,
                    parent,
                    depth,
                } => {
                    if let Some(locator) = spec.locator.as_ref() {
                        locator.validate()?;
                    }

                    let unit_index = spec.locator.as_ref().and_then(BookLocator::unit_index);
                    if let Some(idx) = unit_index
                        && (idx as usize) >= linear_units.len()
                    {
                        return Err(BookNavigationError::UnitIndexOutOfBounds {
                            index: idx,
                            unit_count: linear_units.len(),
                        });
                    }

                    let start_progression = unit_index.and_then(|idx| {
                        compute_progression(&linear_units, total_weight, idx, spec.unit_offset)
                    });

                    let node_index_u32 = u32::try_from(toc.len())
                        .map_err(|_| BookNavigationError::TooManyTocNodes)?;
                    let node_index = toc.len();

                    toc.push(TocNode {
                        parent,
                        depth,
                        subtree_end: 0,
                        title: clean_title(&spec.title),
                        locator: spec.locator,
                        unit_index,
                        unit_offset: spec.unit_offset,
                        start_progression,
                        flags: spec.flags,
                    });

                    stack.push(Frame::Exit { node_index });

                    if !spec.children.is_empty() {
                        let child_depth = depth
                            .checked_add(1)
                            .ok_or(BookNavigationError::MaxDepthExceeded)?;
                        for child in spec.children.into_iter().rev() {
                            stack.push(Frame::Enter {
                                spec: child,
                                parent: Some(node_index_u32),
                                depth: child_depth,
                            });
                        }
                    }
                }
                Frame::Exit { node_index } => {
                    let subtree_end = u32::try_from(toc.len())
                        .map_err(|_| BookNavigationError::TooManyTocNodes)?;
                    toc[node_index].subtree_end = subtree_end;
                }
            }
        }

        Ok(Self {
            toc,
            linear_units,
            total_weight,
        })
    }

    pub fn toc(&self) -> &[TocNode] {
        &self.toc
    }

    pub fn linear_units(&self) -> &[LinearUnit] {
        &self.linear_units
    }

    pub const fn total_weight(&self) -> u64 {
        self.total_weight
    }

    pub fn is_empty(&self) -> bool {
        self.toc.is_empty() && self.linear_units.is_empty()
    }

    pub fn subtree(&self, index: usize) -> Option<&[TocNode]> {
        let node = self.toc.get(index)?;
        let end = (node.subtree_end as usize).min(self.toc.len());
        self.toc.get(index..end)
    }

    pub fn children_of(&self, parent: Option<usize>) -> TocChildrenIter<'_> {
        let (cursor, end) = match parent {
            None => (0, self.toc.len()),
            Some(parent_idx) => match self.toc.get(parent_idx) {
                Some(node) => (
                    parent_idx.saturating_add(1),
                    (node.subtree_end as usize).min(self.toc.len()),
                ),
                None => (0, 0),
            },
        };
        TocChildrenIter {
            toc: &self.toc,
            cursor,
            end,
        }
    }

    pub fn progression_at(
        &self,
        unit_index: u32,
        unit_offset: NormalizedOffset,
    ) -> Option<Progression> {
        compute_progression(&self.linear_units, self.total_weight, unit_index, unit_offset)
    }

    pub fn active_toc_index(
        &self,
        unit_index: u32,
        unit_offset: NormalizedOffset,
    ) -> Option<usize> {
        self.toc
            .iter()
            .enumerate()
            .rfind(|(_, node)| {
                node.unit_index
                    .is_some_and(|node_unit| (node_unit, node.unit_offset) <= (unit_index, unit_offset))
            })
            .map(|(index, _)| index)
    }
}

pub struct TocChildrenIter<'a> {
    toc: &'a [TocNode],
    cursor: usize,
    end: usize,
}

impl<'a> Iterator for TocChildrenIter<'a> {
    type Item = (usize, &'a TocNode);

    fn next(&mut self) -> Option<Self::Item> {
        if self.cursor >= self.end {
            return None;
        }
        let idx = self.cursor;
        let node = &self.toc[idx];
        self.cursor = (node.subtree_end as usize).max(idx + 1);
        Some((idx, node))
    }
}

fn compute_progression(
    linear_units: &[LinearUnit],
    total_weight: u64,
    unit_index: u32,
    unit_offset: NormalizedOffset,
) -> Option<Progression> {
    if total_weight == 0 {
        return None;
    }
    let unit = linear_units.get(unit_index as usize)?;
    if !unit.is_linear {
        return None;
    }
    let position = unit.cumulative_weight as f64
        + unit.weight as f64 * f64::from(unit_offset.value());
    Progression::clamped(position / total_weight as f64)
}

fn clean_title(raw: &str) -> String {
    let mut cleaned = String::with_capacity(raw.len());
    let mut prev_space = true;
    for ch in raw.chars() {
        if ch.is_control() || ch.is_whitespace() {
            if !prev_space {
                cleaned.push(' ');
                prev_space = true;
            }
        } else {
            cleaned.push(ch);
            prev_space = false;
        }
    }
    if cleaned.ends_with(' ') {
        cleaned.pop();
    }
    cleaned
}
