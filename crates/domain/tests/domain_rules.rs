use shiro_domain::library::{
    book::{
        entity::{Book, BookFields},
        errors::BookDomainError,
        file::entity::BookFileType,
        locator::{
            BookAnchor, BookProgression, BookmarkLocator, EpubAnchor, EpubCfi, EpubFragment,
            EpubNavTarget, EpubPointCfi, EpubPointTarget, EpubRangeCfi, EpubResourcePath,
            HighlightLocator, NavigationLocator, NormalizedRect, PdfAnchor, PdfDestination,
            PdfPageRects, PdfSelectionTarget, ReadingUnitOffset, SectionKey,
        },
        navigation::{BookNavigation, ReadingUnitDraft, TocEntryDraft},
        value_objects::{BookHash, BookId, BookStatus, BookTitle, TocEntryFlags},
    },
    tag::{
        entity::TagDefinition,
        errors::TagDomainError,
        tree::TagTree,
        value_objects::{TagColor, TagDeleteStrategy, TagId, TagName, TagValueRule},
    },
};

#[test]
fn book_lifecycle_tracks_dirty_fields_and_enforces_state_invariants() {
    let hash = blake3::hash(b"sample-book-bytes");
    let mut book = Book::new(
        BookId::new(),
        BookTitle::parse("Initial Title").unwrap(),
        BookHash::new(hash),
    );
    assert_eq!(book.status(), BookStatus::Active);

    // Renaming to the same title is rejected.
    assert_eq!(
        book.rename(BookTitle::parse("Initial Title").unwrap())
            .unwrap_err(),
        BookDomainError::TitleUnchanged
    );

    // Renaming updates only TITLE and UPDATED_AT.
    let change = book
        .rename(BookTitle::parse("Renamed Title").unwrap())
        .unwrap();
    assert_eq!(change.fields(), BookFields::TITLE | BookFields::UPDATED_AT);
    assert_eq!(change.entity().title().as_str(), "Renamed Title");

    // Restoring an active book fails.
    assert_eq!(
        book.restore(BookTitle::parse("Renamed Title").unwrap())
            .unwrap_err(),
        BookDomainError::NotDeleted
    );

    // Marking deleted flips status and tracks DELETED_AT | UPDATED_AT.
    let delete_change = book.mark_deleted().unwrap();
    assert_eq!(
        delete_change.fields(),
        BookFields::DELETED_AT | BookFields::UPDATED_AT
    );
    assert!(matches!(book.status(), BookStatus::Deleted { .. }));
    assert_eq!(
        book.mark_deleted().unwrap_err(),
        BookDomainError::AlreadyDeleted
    );
    assert_eq!(
        book.rename(BookTitle::parse("Another").unwrap())
            .unwrap_err(),
        BookDomainError::AlreadyDeleted
    );

    // Restoring with a conflict-resolved title marks TITLE as dirty too.
    let restore_change = book
        .restore(BookTitle::parse("Renamed Title (1)").unwrap())
        .unwrap();
    assert_eq!(
        restore_change.fields(),
        BookFields::DELETED_AT | BookFields::UPDATED_AT | BookFields::TITLE
    );
    assert_eq!(book.status(), BookStatus::Active);

    // Entity inner round-trip via reconstruct preserves every field without any ORM coupling.
    let cloned = book.clone();
    let inner = book.into_inner();
    let reconstructed = Book::reconstruct(
        inner.id,
        inner.title,
        inner.hash,
        inner.created_at,
        inner.updated_at,
        inner.deleted_at,
    );
    assert_eq!(reconstructed, cloned);
}

#[test]
fn tag_value_objects_validate_names_colors_and_regex_rules() {
    assert!(TagName::parse("tag.custom").is_ok());
    assert!(TagName::parse("文学2026").is_ok());
    assert_eq!(TagName::parse("").unwrap_err(), TagDomainError::MissingName);
    assert_eq!(
        TagName::parse("bad name").unwrap_err(),
        TagDomainError::InvalidNameFormat
    );

    assert!(TagColor::parse("#1a2B3c").is_ok());
    assert_eq!(
        TagColor::parse("#fff").unwrap_err(),
        TagDomainError::InvalidColor
    );
    assert_eq!(
        TagColor::parse("1234567").unwrap_err(),
        TagDomainError::InvalidColor
    );

    // Value regex cannot be set when allows_value is false.
    assert_eq!(
        TagValueRule::new(false, Some("^[0-9]+$".to_owned())).unwrap_err(),
        TagDomainError::ValueRegexNotAllowed
    );

    let flag_rule = TagValueRule::new(false, None).unwrap();
    assert!(flag_rule.validate(None).is_ok());
    assert_eq!(
        flag_rule.validate(Some("unexpected")).unwrap_err(),
        TagDomainError::ValueNotAllowed
    );

    // Trailing comment inside (?x) regex is handled safely.
    let regex_rule =
        TagValueRule::new(true, Some("(?x) [0-9]{4} # four-digit year".to_owned())).unwrap();
    assert!(regex_rule.validate(Some("2026")).is_ok());
    assert_eq!(
        regex_rule.validate(Some("2026-extra")).unwrap_err(),
        TagDomainError::ValueRegexMismatch
    );
    assert_eq!(
        regex_rule.validate(Some("")).unwrap_err(),
        TagDomainError::ValueRequired
    );
    assert_eq!(
        TagValueRule::new(true, Some("[unclosed".to_owned())).unwrap_err(),
        TagDomainError::InvalidValueRegex
    );
}

#[test]
fn tag_tree_rejects_cycles_and_plans_cascade_and_promote_deletes() {
    let color = TagColor::parse("#336699").unwrap();
    let rule = TagValueRule::new(false, None).unwrap();

    let root = TagDefinition::new(
        TagName::parse("Root").unwrap(),
        None,
        color.clone(),
        rule.clone(),
    );
    let child = TagDefinition::new(
        TagName::parse("Child").unwrap(),
        Some(root.id().clone()),
        color.clone(),
        rule.clone(),
    );
    let grandchild = TagDefinition::new(
        TagName::parse("Grandchild").unwrap(),
        Some(child.id().clone()),
        color,
        rule,
    );

    let root_id = root.id().clone();
    let child_id = child.id().clone();
    let grandchild_id = grandchild.id().clone();

    let tree = TagTree::new(vec![root, child, grandchild]).unwrap();

    // Moving root under its own descendant is rejected as a cycle.
    assert_eq!(
        tree.plan_move(&root_id, Some(&grandchild_id)).unwrap_err(),
        TagDomainError::InvalidMove
    );
    // Moving to current parent is a no-op invalid move.
    assert_eq!(
        tree.plan_move(&child_id, Some(&root_id)).unwrap_err(),
        TagDomainError::InvalidMove
    );
    // Moving grandchild directly under root succeeds.
    let planned_move = tree.plan_move(&grandchild_id, Some(&root_id)).unwrap();
    assert_eq!(planned_move.id(), &grandchild_id);
    assert_eq!(planned_move.parent_id(), Some(&root_id));

    // Cascade delete emits descendants before ancestors.
    let cascade = tree
        .plan_delete(&root_id, TagDeleteStrategy::Cascade)
        .unwrap();
    assert_eq!(
        cascade.deleted_ids(),
        &[grandchild_id.clone(), child_id.clone(), root_id.clone()]
    );
    assert!(cascade.promoted_children().is_empty());

    // Promote delete re-parents direct children to the deleted node's parent.
    let promote = tree
        .plan_delete(&child_id, TagDeleteStrategy::Promote)
        .unwrap();
    assert_eq!(promote.deleted_ids(), &[child_id]);
    assert_eq!(promote.promoted_children().len(), 1);
    assert_eq!(promote.promoted_children()[0].id(), &grandchild_id);
    assert_eq!(promote.promoted_children()[0].parent_id(), Some(&root_id));
}

#[test]
fn book_navigation_builds_invariants_and_queries() {
    let empty = BookNavigation::empty();
    assert!(empty.is_empty());
    assert!(empty.toc().is_empty());
    assert!(empty.reading_units().is_empty());
    assert_eq!(empty.total_weight(), 0);
    assert_eq!(empty.progression_at(0, ReadingUnitOffset::ZERO), None);
    assert_eq!(empty.active_toc_index(0, ReadingUnitOffset::ZERO), None);

    let href_cover = EpubResourcePath::normalize("/EPUB/../EPUB/cover.xhtml").unwrap();
    assert_eq!(href_cover.as_str(), "EPUB/cover.xhtml");
    let href_ch1 = EpubResourcePath::parse("EPUB/Text/ch1.xhtml").unwrap();
    let href_ch2 = EpubResourcePath::parse("EPUB/Text/ch2.xhtml").unwrap();

    let nav = BookNavigation::build(
        vec![
            TocEntryDraft {
                title: "  Part\tOne \n Overview ".to_owned(),
                locator: None, // pure grouping node
                unit_offset: ReadingUnitOffset::ZERO,
                flags: TocEntryFlags::DEFAULT_COLLAPSED,
                children: vec![
                    TocEntryDraft {
                        title: "Chapter 1".to_owned(),
                        locator: Some(
                            NavigationLocator::epub(
                                Some(1),
                                href_ch1.clone(),
                                EpubNavTarget::Fragment(EpubFragment::parse("intro").unwrap()),
                            )
                            .unwrap(),
                        ),
                        unit_offset: ReadingUnitOffset::ZERO,
                        flags: TocEntryFlags::empty(),
                        children: vec![TocEntryDraft {
                            title: "Section 1.1".to_owned(),
                            locator: Some(
                                NavigationLocator::epub(
                                    Some(1),
                                    href_ch1,
                                    EpubNavTarget::Cfi(
                                        EpubPointCfi::parse("epubcfi(/6/4!/4/2)").unwrap(),
                                    ),
                                )
                                .unwrap(),
                            ),
                            unit_offset: ReadingUnitOffset::new(0.5).unwrap(),
                            flags: TocEntryFlags::empty(),
                            children: vec![],
                        }],
                    },
                    TocEntryDraft {
                        title: "Chapter 2".to_owned(),
                        locator: Some(
                            NavigationLocator::epub(Some(2), href_ch2, EpubNavTarget::Start)
                                .unwrap(),
                        ),
                        unit_offset: ReadingUnitOffset::ZERO,
                        flags: TocEntryFlags::empty(),
                        children: vec![],
                    },
                ],
            },
            TocEntryDraft {
                title: "External Reference".to_owned(),
                locator: None,
                unit_offset: ReadingUnitOffset::ZERO,
                flags: TocEntryFlags::EXTERNAL_LINK,
                children: vec![],
            },
        ],
        vec![
            ReadingUnitDraft {
                is_linear: false,
                weight: 999,
                display_label: Some("  EPUB/cover.xhtml ".to_owned()),
            },
            ReadingUnitDraft {
                is_linear: true,
                weight: 100,
                display_label: Some("EPUB/Text/ch1.xhtml".to_owned()),
            },
            ReadingUnitDraft {
                is_linear: true,
                weight: 300,
                display_label: Some("   ".to_owned()),
            },
        ],
    )
    .unwrap();

    assert!(!nav.is_empty());
    assert_eq!(nav.total_weight(), 400);
    assert_eq!(nav.reading_units().len(), 3);
    assert_eq!(nav.reading_units()[0].index(), 0);
    assert!(!nav.reading_units()[0].is_linear());
    assert_eq!(nav.reading_units()[0].cumulative_weight(), 0);
    assert_eq!(
        nav.reading_units()[0].display_label(),
        Some("EPUB/cover.xhtml")
    );
    assert_eq!(nav.reading_units()[1].cumulative_weight(), 0);
    assert_eq!(nav.reading_units()[2].cumulative_weight(), 100);
    assert_eq!(nav.reading_units()[2].display_label(), None);

    // Pre-order TOC: [0: Part One, 1: Ch1, 2: Sec 1.1, 3: Ch2, 4: External]
    assert_eq!(nav.toc().len(), 5);
    assert_eq!(nav.toc()[0].title(), "Part One Overview");
    assert_eq!(nav.toc()[0].parent(), None);
    assert_eq!(nav.toc()[0].depth(), 0);
    assert_eq!(nav.toc()[0].subtree_end(), 4);
    assert_eq!(nav.toc()[0].flags(), TocEntryFlags::DEFAULT_COLLAPSED);
    assert_eq!(nav.toc()[0].start_progression(), None);

    assert_eq!(nav.toc()[1].parent(), Some(0));
    assert_eq!(nav.toc()[1].depth(), 1);
    assert_eq!(nav.toc()[1].subtree_end(), 3);
    assert_eq!(nav.toc()[1].unit_index(), Some(1));
    assert_eq!(nav.toc()[1].start_progression(), Some(BookProgression::ZERO));

    assert_eq!(nav.toc()[2].parent(), Some(1));
    assert_eq!(nav.toc()[2].depth(), 2);
    assert_eq!(nav.toc()[2].subtree_end(), 3);
    assert_eq!(
        nav.toc()[2].start_progression().unwrap().value(),
        50.0 / 400.0
    );

    assert_eq!(nav.toc()[3].parent(), Some(0));
    assert_eq!(nav.toc()[3].depth(), 1);
    assert_eq!(nav.toc()[3].subtree_end(), 4);
    assert_eq!(
        nav.toc()[3].start_progression().unwrap().value(),
        100.0 / 400.0
    );

    assert_eq!(nav.toc()[4].parent(), None);
    assert_eq!(nav.toc()[4].flags(), TocEntryFlags::EXTERNAL_LINK);

    // children_of & subtree
    let root_children: Vec<_> = nav
        .children_of(None)
        .map(|(idx, node)| (idx, node.title()))
        .collect();
    assert_eq!(
        root_children,
        vec![(0, "Part One Overview"), (4, "External Reference")]
    );
    let part_children: Vec<_> = nav
        .children_of(Some(0))
        .map(|(idx, node)| (idx, node.title()))
        .collect();
    assert_eq!(part_children, vec![(1, "Chapter 1"), (3, "Chapter 2")]);
    assert_eq!(nav.subtree(1).unwrap().len(), 2);

    // progression_at & active_toc_index
    assert_eq!(nav.progression_at(0, ReadingUnitOffset::ZERO), None);
    assert_eq!(
        nav.progression_at(2, ReadingUnitOffset::new(0.5).unwrap())
            .unwrap()
            .value(),
        250.0 / 400.0
    );
    assert_eq!(nav.active_toc_index(0, ReadingUnitOffset::ZERO), None);
    assert_eq!(
        nav.active_toc_index(1, ReadingUnitOffset::new(0.25).unwrap()),
        Some(1)
    );
    assert_eq!(
        nav.active_toc_index(1, ReadingUnitOffset::new(0.5).unwrap()),
        Some(2)
    );
    assert_eq!(
        nav.active_toc_index(2, ReadingUnitOffset::new(0.9).unwrap()),
        Some(3)
    );
}

#[test]
fn book_navigation_validates_value_objects_and_unit_bounds() {
    assert_eq!(
        ReadingUnitOffset::new(-0.01).unwrap_err(),
        BookDomainError::InvalidReadingUnitOffset
    );
    assert_eq!(
        ReadingUnitOffset::new(1.01).unwrap_err(),
        BookDomainError::InvalidReadingUnitOffset
    );
    assert_eq!(
        ReadingUnitOffset::new(f32::NAN).unwrap_err(),
        BookDomainError::InvalidReadingUnitOffset
    );
    assert_eq!(ReadingUnitOffset::clamped(f32::INFINITY), None);
    assert_eq!(
        ReadingUnitOffset::clamped(1.5),
        Some(ReadingUnitOffset::ONE)
    );

    assert_eq!(
        BookProgression::new(-0.1).unwrap_err(),
        BookDomainError::InvalidBookProgression
    );
    assert_eq!(
        BookProgression::new(1.1).unwrap_err(),
        BookDomainError::InvalidBookProgression
    );
    assert_eq!(BookProgression::clamped(f64::NAN), None);

    assert_eq!(
        EpubResourcePath::parse("/leading.xhtml").unwrap_err(),
        BookDomainError::InvalidEpubResourcePath
    );
    assert_eq!(
        EpubResourcePath::parse("OEBPS/../ch1.xhtml").unwrap_err(),
        BookDomainError::InvalidEpubResourcePath
    );
    assert_eq!(
        EpubResourcePath::normalize("../../escape.xhtml").unwrap_err(),
        BookDomainError::InvalidEpubResourcePath
    );
    assert_eq!(
        EpubResourcePath::normalize("a/b#frag").unwrap_err(),
        BookDomainError::InvalidEpubResourcePath
    );
    assert_eq!(
        EpubResourcePath::normalize("").unwrap_err(),
        BookDomainError::InvalidEpubResourcePath
    );
    assert_eq!(
        EpubResourcePath::normalize("/").unwrap_err(),
        BookDomainError::InvalidEpubResourcePath
    );
    assert_eq!(
        EpubResourcePath::normalize("OEBPS/dir/").unwrap_err(),
        BookDomainError::InvalidEpubResourcePath
    );
    assert_eq!(
        EpubResourcePath::normalize("/OEBPS/C%23_Ch1.xhtml")
            .unwrap()
            .as_str(),
        "OEBPS/C%23_Ch1.xhtml"
    );

    assert_eq!(
        PdfDestination::xyz(Some(f32::NAN), None, None, None).unwrap_err(),
        BookDomainError::InvalidPdfDestination
    );
    assert_eq!(
        PdfDestination::xyz(None, None, Some(0.0), None).unwrap_err(),
        BookDomainError::InvalidPdfDestination
    );
    assert_eq!(
        PdfDestination::fit_h(Some(f32::INFINITY), None).unwrap_err(),
        BookDomainError::InvalidPdfDestination
    );

    let href = EpubResourcePath::parse("ch1.xhtml").unwrap();
    assert_eq!(
        EpubFragment::parse("#sec").unwrap_err(),
        BookDomainError::InvalidEpubFragment
    );
    assert_eq!(
        EpubFragment::parse("").unwrap_err(),
        BookDomainError::InvalidEpubFragment
    );
    assert_eq!(
        EpubFragment::parse(" sec ").unwrap_err(),
        BookDomainError::InvalidEpubFragment
    );
    assert_eq!(
        EpubFragment::parse("sec\n1").unwrap_err(),
        BookDomainError::InvalidEpubFragment
    );

    assert_eq!(
        EpubPointCfi::parse("   ").unwrap_err(),
        BookDomainError::InvalidEpubCfi
    );
    assert_eq!(
        EpubPointCfi::parse("epubcfi(/6/2!/4/2,/1:0,/1:5)").unwrap_err(),
        BookDomainError::InvalidEpubCfi
    );
    assert_eq!(
        EpubRangeCfi::parse("epubcfi(/6/2!/4/2/1:0)").unwrap_err(),
        BookDomainError::InvalidEpubCfi
    );

    let pdf_loc = NavigationLocator::pdf(3, PdfDestination::FitPage).unwrap();
    assert_eq!(pdf_loc.unit_index(), Some(3));
    assert_eq!(pdf_loc.default_unit_offset(), ReadingUnitOffset::ZERO);
    assert_eq!(pdf_loc.file_type(), BookFileType::PDF);
    assert_eq!(pdf_loc.section_key(), SectionKey::Pdf(3));
    assert_eq!(pdf_loc.epub_href(), None);

    let epub_loc =
        NavigationLocator::epub(Some(0), href, EpubNavTarget::Start).unwrap();
    assert_eq!(epub_loc.file_type(), BookFileType::EPUB);
    assert_eq!(
        epub_loc.section_key(),
        SectionKey::Epub(EpubResourcePath::parse("ch1.xhtml").unwrap())
    );
    assert_eq!(epub_loc.epub_href().unwrap().as_str(), "ch1.xhtml");

    let oob_err = BookNavigation::build(
        vec![TocEntryDraft {
            title: "Out of Bounds".to_owned(),
            locator: Some(pdf_loc),
            unit_offset: ReadingUnitOffset::ZERO,
            flags: TocEntryFlags::empty(),
            children: vec![],
        }],
        vec![ReadingUnitDraft {
            is_linear: true,
            weight: 1,
            display_label: None,
        }],
    )
    .unwrap_err();
    assert_eq!(
        oob_err,
        BookDomainError::ReadingUnitOutOfBounds {
            index: 3,
            unit_count: 1
        }
    );
}

#[test]
fn active_toc_index_handles_non_monotonic_toc_and_nonlinear_units() {
    let nav = BookNavigation::build(
        vec![
            TocEntryDraft {
                title: "Cover".to_owned(),
                locator: Some(
                    NavigationLocator::epub(
                        Some(0),
                        EpubResourcePath::parse("EPUB/cover.xhtml").unwrap(),
                        EpubNavTarget::Start,
                    )
                    .unwrap(),
                ),
                unit_offset: ReadingUnitOffset::ZERO,
                flags: TocEntryFlags::empty(),
                children: vec![],
            },
            TocEntryDraft {
                title: "Chapter 1".to_owned(),
                locator: Some(
                    NavigationLocator::epub(
                        Some(2),
                        EpubResourcePath::parse("EPUB/ch1.xhtml").unwrap(),
                        EpubNavTarget::Start,
                    )
                    .unwrap(),
                ),
                unit_offset: ReadingUnitOffset::ZERO,
                flags: TocEntryFlags::empty(),
                children: vec![TocEntryDraft {
                    title: "Section 1.0".to_owned(),
                    locator: Some(
                        NavigationLocator::epub(
                            Some(2),
                            EpubResourcePath::parse("EPUB/ch1.xhtml").unwrap(),
                            EpubNavTarget::Fragment(EpubFragment::parse("top").unwrap()),
                        )
                        .unwrap(),
                    ),
                    unit_offset: ReadingUnitOffset::ZERO,
                    flags: TocEntryFlags::empty(),
                    children: vec![],
                }],
            },
            TocEntryDraft {
                title: "Chapter 2".to_owned(),
                locator: Some(
                    NavigationLocator::epub(
                        Some(3),
                        EpubResourcePath::parse("EPUB/ch2.xhtml").unwrap(),
                        EpubNavTarget::Start,
                    )
                    .unwrap(),
                ),
                unit_offset: ReadingUnitOffset::ZERO,
                flags: TocEntryFlags::empty(),
                children: vec![],
            },
            TocEntryDraft {
                title: "Frontmatter Note".to_owned(),
                locator: Some(
                    NavigationLocator::epub(
                        Some(1),
                        EpubResourcePath::parse("EPUB/front.xhtml").unwrap(),
                        EpubNavTarget::Start,
                    )
                    .unwrap(),
                ),
                unit_offset: ReadingUnitOffset::ZERO,
                flags: TocEntryFlags::empty(),
                children: vec![],
            },
        ],
        vec![
            ReadingUnitDraft {
                is_linear: false,
                weight: 10,
                display_label: Some("EPUB/cover.xhtml".to_owned()),
            },
            ReadingUnitDraft {
                is_linear: true,
                weight: 100,
                display_label: Some("EPUB/front.xhtml".to_owned()),
            },
            ReadingUnitDraft {
                is_linear: true,
                weight: 100,
                display_label: Some("EPUB/ch1.xhtml".to_owned()),
            },
            ReadingUnitDraft {
                is_linear: true,
                weight: 100,
                display_label: Some("EPUB/ch2.xhtml".to_owned()),
            },
            ReadingUnitDraft {
                is_linear: true,
                weight: 100,
                display_label: Some("EPUB/ch3.xhtml".to_owned()),
            },
        ],
    )
    .unwrap();

    // Non-monotonic trailing frontmatter node (unit 1) must not shadow Chapter 2 (unit 3).
    assert_eq!(nav.active_toc_index(4, ReadingUnitOffset::ZERO), Some(3));
    // Querying unit 1 matches the frontmatter note despite appearing last in TOC.
    assert_eq!(
        nav.active_toc_index(1, ReadingUnitOffset::new(0.5).unwrap()),
        Some(4)
    );
    // Identical position (2, 0.0) selects deeper/later pre-order node (Section 1.0).
    assert_eq!(nav.active_toc_index(2, ReadingUnitOffset::ZERO), Some(2));
    // Non-linear cover matches only when directly on unit 0.
    assert_eq!(nav.active_toc_index(0, ReadingUnitOffset::ZERO), Some(0));
    // Out-of-bounds unit index returns None.
    assert_eq!(nav.active_toc_index(99, ReadingUnitOffset::ZERO), None);
}

#[test]
fn book_anchor_validates_cfi_variants_pdf_selections_and_conversions() {
    // 1. Point CFI, section base CFI, and text assertions with commas/escapes.
    assert!(EpubPointCfi::parse("epubcfi(/6/4[chapter-1]!)").is_ok());
    assert!(EpubPointCfi::parse("epubcfi(/6/4[ch1]!/4/2/1:12[a,b^,c])").is_ok());
    assert!(EpubCfi::parse("epubcfi(/6/4!/4/2/1:0)").unwrap().is_point());

    for invalid in [
        "",
        " epubcfi(/6/4!)",
        "epubcfi(/6/4!)\n",
        "epubcfi()",
        "epubcfi(6/4)",
        "epubcfi(/6/4!/4/-1)",
        "epubcfi(/6/4[unclosed!)",
        "epubcfi(/6/4[[nested]])",
        "epubcfi(/6/4!/4 /2)",
        "epubcfi(/6/4!/4^)",
        "epubcfi(/6/4!/4,/2)",
        "epubcfi(/6/4!/4,,)",
        "epubcfi(/6/4!/4,bad,/2)",
        "epubcfi(/6/4!/4,/2,bad)",
        "epubcfi(/6/4!/4,/2,/3,/4)",
    ] {
        assert_eq!(
            EpubCfi::parse(invalid).unwrap_err(),
            BookDomainError::InvalidEpubCfi,
            "expected InvalidEpubCfi for {invalid:?}"
        );
    }

    // 2. Range CFI parsing & start/end point extraction.
    let range = EpubRangeCfi::parse("epubcfi(/6/4[ch1]!/4/2,/1:0,/3:15)").unwrap();
    assert_eq!(
        range.start_point().as_str(),
        "epubcfi(/6/4[ch1]!/4/2/1:0)"
    );
    assert_eq!(
        range.end_point().as_str(),
        "epubcfi(/6/4[ch1]!/4/2/3:15)"
    );

    let same_node_range = EpubRangeCfi::parse("epubcfi(/6/4!/4/2/1,:2,:9)").unwrap();
    assert_eq!(
        same_node_range.start_point().as_str(),
        "epubcfi(/6/4!/4/2/1:2)"
    );
    assert_eq!(
        same_node_range.end_point().as_str(),
        "epubcfi(/6/4!/4/2/1:9)"
    );

    let half_empty_range = EpubRangeCfi::parse("epubcfi(/6/4!/4,,/2:5)").unwrap();
    assert_eq!(half_empty_range.start_point().as_str(), "epubcfi(/6/4!/4)");
    assert_eq!(half_empty_range.end_point().as_str(), "epubcfi(/6/4!/4/2:5)");

    let wrapped: EpubCfi = range.clone().into();
    assert!(wrapped.is_range());
    assert!(!wrapped.is_point());
    assert!(wrapped.clone().into_point().is_err());
    assert_eq!(wrapped.into_range().unwrap(), range);

    // 3. HighlightLocator <-> BookmarkLocator <-> NavigationLocator conversions (EPUB).
    let epub_hl = HighlightLocator::epub(
        Some(1),
        EpubResourcePath::parse("EPUB/ch1.xhtml").unwrap(),
        range,
    )
    .unwrap();
    assert_eq!(
        epub_hl.covered_section_keys(),
        vec![SectionKey::Epub(
            EpubResourcePath::parse("EPUB/ch1.xhtml").unwrap()
        )]
    );
    let epub_bm = epub_hl.to_bookmark_locator();
    assert_eq!(epub_bm.default_unit_offset(), ReadingUnitOffset::ZERO);
    assert_eq!(
        epub_bm.to_navigation_locator(),
        NavigationLocator::epub(
            Some(1),
            EpubResourcePath::parse("EPUB/ch1.xhtml").unwrap(),
            EpubNavTarget::Cfi(EpubPointCfi::parse("epubcfi(/6/4[ch1]!/4/2/1:0)").unwrap()),
        )
        .unwrap()
    );

    let offset_bm = BookmarkLocator::epub(
        Some(0),
        EpubResourcePath::parse("EPUB/cover.xhtml").unwrap(),
        EpubPointTarget::UnitOffset(ReadingUnitOffset::new(0.4).unwrap()),
    )
    .unwrap();
    assert_eq!(
        offset_bm.default_unit_offset(),
        ReadingUnitOffset::new(0.4).unwrap()
    );
    assert_eq!(
        offset_bm.to_navigation_locator(),
        NavigationLocator::epub(
            Some(0),
            EpubResourcePath::parse("EPUB/cover.xhtml").unwrap(),
            EpubNavTarget::Start,
        )
        .unwrap()
    );

    // 4. NormalizedRect, PdfPageRects, PdfSelectionTarget & HighlightLocator (PDF).
    assert_eq!(
        NormalizedRect::new(-0.1, 0.1, 0.2, 0.2).unwrap_err(),
        BookDomainError::InvalidNormalizedRect
    );
    assert_eq!(
        NormalizedRect::new(0.1, 0.1, 0.0, 0.2).unwrap_err(),
        BookDomainError::InvalidNormalizedRect
    );
    assert_eq!(
        NormalizedRect::new(0.6, 0.2, 0.5, 0.2).unwrap_err(),
        BookDomainError::InvalidNormalizedRect
    );
    assert_eq!(
        NormalizedRect::new(0.1, 0.8, 0.2, f32::NAN).unwrap_err(),
        BookDomainError::InvalidNormalizedRect
    );
    assert_eq!(
        PdfPageRects::new(2, vec![]).unwrap_err(),
        BookDomainError::InvalidPdfSelection
    );
    assert_eq!(
        PdfSelectionTarget::new(vec![], vec![]).unwrap_err(),
        BookDomainError::InvalidPdfSelection
    );

    let r1 = NormalizedRect::new(0.1, 0.45, 0.7, 0.05).unwrap();
    let r2 = NormalizedRect::new(0.1, 0.25, 0.5, 0.05).unwrap();
    let r_next = NormalizedRect::new(0.1, 0.1, 0.6, 0.04).unwrap();
    let p4 = PdfPageRects::new(4, vec![r_next]).unwrap();
    let p3 = PdfPageRects::new(3, vec![r_next]).unwrap();

    // Extra pages must strictly increase within target and relative to anchor start page.
    assert_eq!(
        PdfSelectionTarget::new(vec![r1], vec![p4.clone(), p3.clone()]).unwrap_err(),
        BookDomainError::InvalidPdfSelection
    );
    let pdf_sel = PdfSelectionTarget::new(vec![r1, r2], vec![p3, p4]).unwrap();
    assert_eq!(
        HighlightLocator::pdf(3, pdf_sel.clone()).unwrap_err(),
        BookDomainError::InvalidPdfSelection
    );

    let pdf_hl = HighlightLocator::pdf(2, pdf_sel).unwrap();
    assert_eq!(
        pdf_hl.covered_section_keys(),
        vec![SectionKey::Pdf(2), SectionKey::Pdf(3), SectionKey::Pdf(4)]
    );
    assert_eq!(
        pdf_hl.to_bookmark_locator(),
        BookAnchor::Pdf(PdfAnchor {
            page_index: 2,
            target: PdfDestination::FitH {
                top: None,
                normalized_top: Some(ReadingUnitOffset::new(0.25).unwrap()),
            },
        })
    );
    assert_eq!(
        pdf_hl.to_navigation_locator().default_unit_offset(),
        ReadingUnitOffset::new(0.25).unwrap()
    );

    // map_targets preserves container fields cleanly.
    let mapped = BookAnchor::<u8, u8>::Epub(EpubAnchor {
        spine_index: Some(0),
        href: EpubResourcePath::parse("a.xhtml").unwrap(),
        target: 1u8,
    })
    .map_targets(|n| n + 1, |n| n);
    assert!(matches!(mapped, BookAnchor::Epub(EpubAnchor { target: 2, .. })));
}

#[test]
fn tag_definition_reconstruct_validates_self_parent_and_preserves_fields() {
    let tag_id = TagId::new();
    let parent_id = TagId::new();
    let name = TagName::parse("Fiction").unwrap();
    let color = TagColor::parse("#112233").unwrap();
    let rule = TagValueRule::new(true, Some("^[a-z]+$".to_owned())).unwrap();
    let created_at = jiff::Timestamp::from_second(1_700_000_000).unwrap();
    let updated_at = jiff::Timestamp::from_second(1_700_000_100).unwrap();

    assert_eq!(
        TagDefinition::reconstruct(
            tag_id.clone(),
            name.clone(),
            Some(tag_id.clone()),
            color.clone(),
            rule.clone(),
            created_at,
            updated_at,
        )
        .unwrap_err(),
        TagDomainError::InvalidTree
    );

    let reconstructed = TagDefinition::reconstruct(
        tag_id.clone(),
        name.clone(),
        Some(parent_id.clone()),
        color.clone(),
        rule.clone(),
        created_at,
        updated_at,
    )
    .unwrap();
    assert_eq!(reconstructed.id(), &tag_id);
    assert_eq!(reconstructed.name(), &name);
    assert_eq!(reconstructed.parent_id(), Some(&parent_id));
    assert_eq!(reconstructed.color(), &color);
    assert_eq!(reconstructed.value_rule(), &rule);
    assert_eq!(reconstructed.created_at(), created_at);
    assert_eq!(reconstructed.updated_at(), updated_at);
}

#[test]
fn zero_copy_value_objects_and_navigation_preserve_string_buffers() {
    let raw_title = String::from("Zero Copy Title");
    let raw_title_ptr = raw_title.as_ptr();
    let title_out = BookTitle::parse(raw_title).unwrap().into_inner();
    assert_eq!(title_out.as_ptr(), raw_title_ptr);

    let raw_tag_name = String::from("tag.zero.copy");
    let raw_tag_name_ptr = raw_tag_name.as_ptr();
    let tag_name_out = TagName::parse(raw_tag_name).unwrap().into_inner();
    assert_eq!(tag_name_out.as_ptr(), raw_tag_name_ptr);

    let raw_tag_color = String::from("#AABBCC");
    let raw_tag_color_ptr = raw_tag_color.as_ptr();
    let tag_color_out = TagColor::parse(raw_tag_color).unwrap().into_inner();
    assert_eq!(tag_color_out.as_ptr(), raw_tag_color_ptr);

    let raw_regex = String::from("^[0-9]+$");
    let raw_regex_ptr = raw_regex.as_ptr();
    let (allows_value, regex_out) = TagValueRule::new(true, Some(raw_regex)).unwrap().into_inner();
    assert!(allows_value);
    let regex_out = regex_out.unwrap();
    assert_eq!(regex_out.as_ptr(), raw_regex_ptr);

    let raw_path = String::from("OEBPS/ch1.xhtml");
    let raw_path_ptr = raw_path.as_ptr();
    let path_vo = EpubResourcePath::parse(raw_path).unwrap();

    let raw_fragment = String::from("sec-1");
    let raw_fragment_ptr = raw_fragment.as_ptr();
    let fragment_vo = EpubFragment::parse(raw_fragment).unwrap();

    let raw_point_cfi = String::from("epubcfi(/6/4!/4/2/1:0)");
    let raw_point_cfi_ptr = raw_point_cfi.as_ptr();
    let point_cfi_out = EpubPointCfi::parse(raw_point_cfi).unwrap().into_inner();
    assert_eq!(point_cfi_out.as_ptr(), raw_point_cfi_ptr);

    let raw_range_cfi = String::from("epubcfi(/6/4!/4/2,/1:0,/1:5)");
    let raw_range_cfi_ptr = raw_range_cfi.as_ptr();
    let range_cfi_out = EpubRangeCfi::parse(raw_range_cfi).unwrap().into_inner();
    assert_eq!(range_cfi_out.as_ptr(), raw_range_cfi_ptr);

    let raw_cfi = String::from("epubcfi(/6/4!/4/2)");
    let raw_cfi_ptr = raw_cfi.as_ptr();
    let cfi_out = EpubCfi::parse(raw_cfi).unwrap().into_inner();
    assert_eq!(cfi_out.as_ptr(), raw_cfi_ptr);

    let nav = BookNavigation::build(
        vec![TocEntryDraft {
            title: "Chapter 1".to_owned(),
            locator: Some(
                NavigationLocator::epub(Some(0), path_vo, EpubNavTarget::Fragment(fragment_vo))
                    .unwrap(),
            ),
            unit_offset: ReadingUnitOffset::ZERO,
            flags: TocEntryFlags::empty(),
            children: vec![],
        }],
        vec![ReadingUnitDraft {
            is_linear: true,
            weight: 10,
            display_label: Some("OEBPS/ch1.xhtml".to_owned()),
        }],
    )
    .unwrap();
    let toc_title_ptr = nav.toc()[0].title().as_ptr();
    let unit_label_ptr = nav.reading_units()[0].display_label().unwrap().as_ptr();
    let mut nav_inner = nav.into_inner();
    let toc_inner = nav_inner.toc.remove(0).into_inner();
    let unit_inner = nav_inner.reading_units.remove(0).into_inner();
    let unit_label_out = unit_inner.display_label.unwrap();
    assert_eq!(toc_inner.title.as_ptr(), toc_title_ptr);
    assert_eq!(unit_label_out.as_ptr(), unit_label_ptr);
    match toc_inner.locator.unwrap() {
        BookAnchor::Epub(epub) => {
            let href_out = epub.href.into_inner();
            assert_eq!(href_out.as_ptr(), raw_path_ptr);
            match epub.target {
                EpubNavTarget::Fragment(fragment) => {
                    let fragment_out = fragment.into_inner();
                    assert_eq!(fragment_out.as_ptr(), raw_fragment_ptr);
                }
                other => panic!("unexpected target: {other:?}"),
            }
        }
        other => panic!("unexpected locator: {other:?}"),
    }
}

#[cfg(feature = "identity")]
#[test]
fn identity_token_hash_and_password_hash_validate_and_preserve_buffers() {
    use shiro_domain::identity::{
        session::{SessionDomainError, TokenHash},
        user::{
            entity::User,
            value_objects::{UserId, UserName, UserPasswordHash},
        },
    };

    assert_eq!(
        TokenHash::parse("").unwrap_err(),
        SessionDomainError::MissingTokenHash
    );
    assert_eq!(
        TokenHash::parse("abc").unwrap_err(),
        SessionDomainError::InvalidTokenHash
    );
    assert_eq!(
        TokenHash::parse("g".repeat(64)).unwrap_err(),
        SessionDomainError::InvalidTokenHash
    );

    let raw_token_hash = "0123456789abcdef".repeat(4);
    let raw_token_hash_ptr = raw_token_hash.as_ptr();
    let token_hash_out = TokenHash::parse(raw_token_hash).unwrap().into_inner();
    assert_eq!(token_hash_out.as_ptr(), raw_token_hash_ptr);

    let raw_phc = String::from(
        "$argon2id$v=19$m=1024,t=1,p=1$c2FsdHNhbHRzYWx0$YWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWE",
    );
    let raw_phc_ptr = raw_phc.as_ptr();
    let password_hash = UserPasswordHash::parse(raw_phc).unwrap();

    let user_id = UserId::new();
    let user_name = UserName::parse("Alice").unwrap();
    let created_at = jiff::Timestamp::from_second(1_700_000_000).unwrap();
    let updated_at = jiff::Timestamp::from_second(1_700_000_100).unwrap();
    let user = User::reconstruct(
        user_id.clone(),
        user_name.clone(),
        password_hash,
        created_at,
        updated_at,
    );
    assert_eq!(user.id(), &user_id);
    assert_eq!(user.name(), &user_name);
    assert_eq!(user.created_at(), created_at);
    assert_eq!(user.updated_at(), updated_at);
    let password_hash_out = user.into_inner().password_hash.into_inner();
    assert_eq!(password_hash_out.as_ptr(), raw_phc_ptr);
}
