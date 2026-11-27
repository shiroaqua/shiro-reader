use super::{
    errors::BookDomainError,
    locator::{BookProgression, NavigationLocator, ReadingUnitOffset},
    value_objects::TocEntryFlags,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadingUnitDraft {
    pub is_linear: bool,
    pub weight: u64,
    pub display_label: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TocEntryDraft {
    pub title: String,
    pub locator: Option<NavigationLocator>,
    pub unit_offset: ReadingUnitOffset,
    pub flags: TocEntryFlags,
    pub children: Vec<TocEntryDraft>,
}

/// Physical reading unit forming a continuous monotonic progression axis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadingUnit {
    index: u32,
    is_linear: bool,
    weight: u64,
    cumulative_weight: u64,
    display_label: Option<String>,
}

impl ReadingUnit {
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

/// Logical TOC entry stored in a pre-order flattened array (array index is entry ID).
#[derive(Debug, Clone, PartialEq)]
pub struct TocEntry {
    parent: Option<u32>,
    depth: u16,
    subtree_end: u32,
    title: String,
    locator: Option<NavigationLocator>,
    unit_index: Option<u32>,
    unit_offset: ReadingUnitOffset,
    start_progression: Option<BookProgression>,
    flags: TocEntryFlags,
}

impl TocEntry {
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

    pub const fn locator(&self) -> Option<&NavigationLocator> {
        self.locator.as_ref()
    }

    pub const fn unit_index(&self) -> Option<u32> {
        self.unit_index
    }

    pub const fn unit_offset(&self) -> ReadingUnitOffset {
        self.unit_offset
    }

    pub const fn start_progression(&self) -> Option<BookProgression> {
        self.start_progression
    }

    pub const fn flags(&self) -> TocEntryFlags {
        self.flags
    }
}

/// Unified navigation tree and progression index for a book.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BookNavigation {
    toc: Vec<TocEntry>,
    reading_units: Vec<ReadingUnit>,
    total_weight: u64,
}

impl BookNavigation {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn build(
        toc_roots: Vec<TocEntryDraft>,
        unit_drafts: Vec<ReadingUnitDraft>,
    ) -> Result<Self, BookDomainError> {
        u32::try_from(unit_drafts.len()).map_err(|_| BookDomainError::TooManyReadingUnits)?;

        let mut reading_units = Vec::with_capacity(unit_drafts.len());
        let mut running_weight = 0u64;
        for (i, draft) in unit_drafts.into_iter().enumerate() {
            let display_label = draft
                .display_label
                .map(|label| label.trim().to_owned())
                .filter(|label| !label.is_empty());
            let cumulative_weight = running_weight;
            if draft.is_linear {
                running_weight = running_weight.saturating_add(draft.weight);
            }
            reading_units.push(ReadingUnit {
                index: i as u32,
                is_linear: draft.is_linear,
                weight: draft.weight,
                cumulative_weight,
                display_label,
            });
        }
        let total_weight = running_weight;

        enum Frame {
            Enter {
                draft: TocEntryDraft,
                parent: Option<u32>,
                depth: u16,
            },
            Exit {
                entry_index: usize,
            },
        }

        let mut toc = Vec::new();
        let mut stack: Vec<Frame> = toc_roots
            .into_iter()
            .rev()
            .map(|draft| Frame::Enter {
                draft,
                parent: None,
                depth: 0,
            })
            .collect();

        while let Some(frame) = stack.pop() {
            match frame {
                Frame::Enter {
                    draft,
                    parent,
                    depth,
                } => {
                    if let Some(locator) = draft.locator.as_ref() {
                        locator.validate()?;
                    }

                    let unit_index = draft.locator.as_ref().and_then(NavigationLocator::unit_index);
                    if let Some(idx) = unit_index
                        && (idx as usize) >= reading_units.len()
                    {
                        return Err(BookDomainError::ReadingUnitOutOfBounds {
                            index: idx,
                            unit_count: reading_units.len(),
                        });
                    }

                    let start_progression = unit_index.and_then(|idx| {
                        compute_progression(&reading_units, total_weight, idx, draft.unit_offset)
                    });

                    let entry_index_u32 = u32::try_from(toc.len())
                        .map_err(|_| BookDomainError::TooManyTocEntries)?;
                    let entry_index = toc.len();

                    toc.push(TocEntry {
                        parent,
                        depth,
                        subtree_end: 0,
                        title: clean_title(&draft.title),
                        locator: draft.locator,
                        unit_index,
                        unit_offset: draft.unit_offset,
                        start_progression,
                        flags: draft.flags,
                    });

                    stack.push(Frame::Exit { entry_index });

                    if !draft.children.is_empty() {
                        let child_depth = depth
                            .checked_add(1)
                            .ok_or(BookDomainError::TocMaxDepthExceeded)?;
                        for child in draft.children.into_iter().rev() {
                            stack.push(Frame::Enter {
                                draft: child,
                                parent: Some(entry_index_u32),
                                depth: child_depth,
                            });
                        }
                    }
                }
                Frame::Exit { entry_index } => {
                    let subtree_end = u32::try_from(toc.len())
                        .map_err(|_| BookDomainError::TooManyTocEntries)?;
                    toc[entry_index].subtree_end = subtree_end;
                }
            }
        }

        Ok(Self {
            toc,
            reading_units,
            total_weight,
        })
    }

    pub fn toc(&self) -> &[TocEntry] {
        &self.toc
    }

    pub fn reading_units(&self) -> &[ReadingUnit] {
        &self.reading_units
    }

    pub const fn total_weight(&self) -> u64 {
        self.total_weight
    }

    pub fn is_empty(&self) -> bool {
        self.toc.is_empty() && self.reading_units.is_empty()
    }

    pub fn subtree(&self, index: usize) -> Option<&[TocEntry]> {
        let entry = self.toc.get(index)?;
        let end = (entry.subtree_end as usize).min(self.toc.len());
        self.toc.get(index..end)
    }

    pub fn children_of(&self, parent: Option<usize>) -> TocEntryChildren<'_> {
        let (cursor, end) = match parent {
            None => (0, self.toc.len()),
            Some(parent_idx) => match self.toc.get(parent_idx) {
                Some(entry) => (
                    parent_idx.saturating_add(1),
                    (entry.subtree_end as usize).min(self.toc.len()),
                ),
                None => (0, 0),
            },
        };
        TocEntryChildren {
            toc: &self.toc,
            cursor,
            end,
        }
    }

    pub fn progression_at(
        &self,
        unit_index: u32,
        unit_offset: ReadingUnitOffset,
    ) -> Option<BookProgression> {
        compute_progression(&self.reading_units, self.total_weight, unit_index, unit_offset)
    }

    pub fn active_toc_index(
        &self,
        unit_index: u32,
        unit_offset: ReadingUnitOffset,
    ) -> Option<usize> {
        let current_unit = self.reading_units.get(unit_index as usize)?;
        let target = (unit_index, unit_offset);
        let mut best: Option<(usize, (u32, ReadingUnitOffset))> = None;

        for (index, entry) in self.toc.iter().enumerate() {
            let Some(entry_unit) = entry.unit_index else {
                continue;
            };
            let Some(unit) = self.reading_units.get(entry_unit as usize) else {
                continue;
            };
            if current_unit.is_linear {
                if !unit.is_linear {
                    continue;
                }
            } else if entry_unit != unit_index {
                continue;
            }

            let pos = (entry_unit, entry.unit_offset);
            if pos <= target && best.is_none_or(|(_, best_pos)| pos >= best_pos) {
                best = Some((index, pos));
            }
        }

        best.map(|(index, _)| index)
    }
}

pub struct TocEntryChildren<'a> {
    toc: &'a [TocEntry],
    cursor: usize,
    end: usize,
}

impl<'a> Iterator for TocEntryChildren<'a> {
    type Item = (usize, &'a TocEntry);

    fn next(&mut self) -> Option<Self::Item> {
        if self.cursor >= self.end {
            return None;
        }
        let idx = self.cursor;
        let entry = &self.toc[idx];
        self.cursor = (entry.subtree_end as usize).max(idx + 1);
        Some((idx, entry))
    }
}

fn compute_progression(
    reading_units: &[ReadingUnit],
    total_weight: u64,
    unit_index: u32,
    unit_offset: ReadingUnitOffset,
) -> Option<BookProgression> {
    if total_weight == 0 {
        return None;
    }
    let unit = reading_units.get(unit_index as usize)?;
    if !unit.is_linear {
        return None;
    }
    let position = unit.cumulative_weight as f64
        + unit.weight as f64 * f64::from(unit_offset.value());
    BookProgression::clamped(position / total_weight as f64)
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
