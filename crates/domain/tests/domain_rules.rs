use shiro_domain::library::{
    book::{
        entity::{Book, BookFields},
        errors::BookDomainError,
        locator::{
            BookAnchor, BookProgression, BookmarkLocator, EpubAnchor, EpubCfi, EpubFragment,
            EpubNavTarget, EpubPointCfi, EpubPointTarget, EpubRangeCfi, EpubResourcePath,
            HighlightLocator, NavigationLocator, NormalizedRect, PdfAnchor, PdfDestination,
            PdfPageRects, PdfSelectionTarget, ReadingUnitOffset, SectionKey,
        },
        navigation::{BookNavigation, ReadingUnitDraft, TocEntryDraft},
        value_objects::{
            BookAuthor, BookDescription, BookFileType, BookHash, BookId, BookIdentifier,
            BookIdentifierScheme, BookLanguage, BookPublishedDate, BookPublisher, BookStatus,
            BookSubtitle, BookTitle, MAX_BOOK_AUTHORS, TocEntryFlags,
        },
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

#[test]
fn book_metadata_value_objects_validate_and_canonicalize_correctly() {
    // Publisher
    assert_eq!(
        BookPublisher::parse("Shiro Press").unwrap().as_str(),
        "Shiro Press"
    );
    assert_eq!(
        BookPublisher::parse("").unwrap_err(),
        BookDomainError::InvalidPublisher
    );
    assert_eq!(
        BookPublisher::parse(" Shiro ").unwrap_err(),
        BookDomainError::InvalidPublisher
    );
    assert_eq!(
        BookPublisher::parse("Shiro\nPress").unwrap_err(),
        BookDomainError::InvalidPublisher
    );
    assert_eq!(
        BookPublisher::parse("x".repeat(257)).unwrap_err(),
        BookDomainError::InvalidPublisher
    );

    // Description (allows newlines & tabs, rejects other control chars)
    assert_eq!(
        BookDescription::parse("Line 1\n\tLine 2").unwrap().as_str(),
        "Line 1\n\tLine 2"
    );
    assert_eq!(
        BookDescription::parse("").unwrap_err(),
        BookDomainError::InvalidDescription
    );
    assert_eq!(
        BookDescription::parse("  padded  ").unwrap_err(),
        BookDomainError::InvalidDescription
    );
    assert_eq!(
        BookDescription::parse("bad\x00char").unwrap_err(),
        BookDomainError::InvalidDescription
    );
    assert_eq!(
        BookDescription::parse("a".repeat(16_385)).unwrap_err(),
        BookDomainError::InvalidDescription
    );

    // Published date (YYYY / YYYY-MM / YYYY-MM-DD)
    let y = BookPublishedDate::parse("2024").unwrap();
    assert_eq!(y.year(), 2024);
    assert_eq!(y.month(), None);
    assert_eq!(y.day(), None);
    assert_eq!(y.to_string(), "2024");

    let ym = BookPublishedDate::parse("2024-05").unwrap();
    assert_eq!(ym.year(), 2024);
    assert_eq!(ym.month(), Some(5));
    assert_eq!(ym.day(), None);
    assert_eq!(ym.to_string(), "2024-05");

    let ymd = BookPublishedDate::parse("2024-02-29").unwrap();
    assert_eq!(ymd.year(), 2024);
    assert_eq!(ymd.month(), Some(2));
    assert_eq!(ymd.day(), Some(29));
    assert_eq!(ymd.to_string(), "2024-02-29");

    for bad_date in [
        "",
        "202",
        "2024-5",
        "2024-13",
        "2023-02-29",
        "2024/05/01",
        "2024-05-01T00:00:00Z",
        "0000",
    ] {
        assert_eq!(
            BookPublishedDate::parse(bad_date).unwrap_err(),
            BookDomainError::InvalidPublishedDate,
            "date {bad_date:?} should fail"
        );
    }

    // Language (BCP 47 + RFC 5646 Section 2.1.1 canonical casing)
    for (raw, expected) in [
        ("EN", "en"),
        ("zh-cn", "zh-CN"),
        ("zh-hans-cn", "zh-Hans-CN"),
        ("zh-cmn-hans-cn", "zh-cmn-Hans-CN"),
        ("es-419", "es-419"),
        ("de-CH-1996", "de-CH-1996"),
        ("en-u-CA-gregory-x-Local", "en-u-ca-gregory-x-local"),
    ] {
        let parsed = BookLanguage::parse(raw).unwrap();
        assert_eq!(parsed.as_str(), expected);
        assert_eq!(BookLanguage::parse(parsed.as_str()).unwrap(), parsed);
    }
    for bad_lang in [
        "",
        "e",
        "english",
        "en_US",
        "en--US",
        "en-",
        "-en",
        "123",
        "en-u",
        "en-1996-1996",
        "en-u-a-u-b",
        "en-x",
    ] {
        assert_eq!(
            BookLanguage::parse(bad_lang).unwrap_err(),
            BookDomainError::InvalidLanguage,
            "language {bad_lang:?} should fail"
        );
    }
}

#[test]
fn book_identifier_validates_all_international_and_national_schemes_with_round_trip() {
    use BookIdentifierScheme::*;

    let valid_cases = [
        ("978-0-306-40615-7", Isbn13, "9780306406157"),
        ("ISBN: 978 0 306 40615 7", Isbn13, "9780306406157"),
        ("0-306-40615-2", Isbn10, "0306406152"),
        ("ISBN-10:0-8044-2957-x", Isbn10, "080442957X"),
        ("0378-5955", Issn, "0378-5955"),
        ("ISSN:20493630", Issn, "2049-3630"),
        ("2434-561x", Issn, "2434-561X"),
        ("977-0378595-00-2", Issn13, "9770378595002"),
        ("979-0-060-11561-5", Ismn13, "9790060115615"),
        ("M-060-11561-5", Ismn10, "M060115615"),
        ("10.1000/182", Doi, "DOI:10.1000/182"),
        ("https://doi.org/10.1007/978-3-030-12345-6", Doi, "DOI:10.1007/978-3-030-12345-6"),
        ("0A9-2002-12B4A105-7", Istc, "ISTC:0A9-2002-12B4A105-7"),
        ("ISTC: a022009000004bea", Istc, "ISTC:A02-2009-000004BE-A"),
        ("B159-D8FA-0124-0000-K", Isan, "ISAN:B159-D8FA-0124-0000-K"),
        (
            "0000-0001-8947-0000-8-0000-0000-D",
            Isan,
            "ISAN:0000-0001-8947-0000-8-0000-0000-D",
        ),
        ("urn:nbn:DE:bvb:19-146642", UrnNbn, "urn:nbn:de:bvb:19-146642"),
        ("CN 11-0106", Cn, "CN:11-0106"),
        ("CN44-1661/x", Cn, "CN:44-1661/X"),
        ("CN: 81-0001/（j）", Cn, "CN:81-0001/(J)"),
        ("10018·123", Csbn, "CSBN:10018\u{00B7}123"),
        ("统一书号: T15031·12", Csbn, "CSBN:T15031\u{00B7}12"),
        ("CIP: 2023123456", Cip, "CIP:2023123456"),
        ("CIP:(2024)第012345号", Cip, "CIP:2024012345"),
        ("LCCN:n78-890351", Lccn, "LCCN:n78890351"),
        ("LCCN:2001-2", Lccn, "LCCN:2001000002"),
        ("JP20230001", Jpnb, "JP:20230001"),
        ("JPNB: 2023000123", Jpnb, "JP:2023000123"),
        ("4910-01511-054-9", JpMagazine, "JPMAG:4910015110549"),
        ("JPMAG:01511-05", JpMagazine, "JPMAG:01511-05"),
        ("GPN:1011200123", Gpn, "GPN:1011200123"),
        ("ECN-1234-2023-001-000000001", Ecn, "ECN:1234-2023-001-000000001"),
        ("ECN: 12342023001000000002", Ecn, "ECN:1234-2023-001-000000002"),
        ("SBN:306-40615-2", Sbn, "SBN:306406152"),
        ("GB9912345", Bnb, "BNB:GB9912345"),
        ("BNB:gba123456", Bnb, "BNB:GBA123456"),
        ("DNB:123456789X", Dnb, "DNB:123456789X"),
        ("ark:/12148/cb30000001z", Bnf, "BNF:cb30000001z"),
        ("BNF:FRBNF30000001", Bnf, "BNF:FRBNF30000001"),
    ];

    for (raw, expected_scheme, expected_canonical) in valid_cases {
        let parsed = BookIdentifier::parse(raw)
            .unwrap_or_else(|e| panic!("expected {raw:?} to parse, got {e:?}"));
        assert_eq!(parsed.scheme(), expected_scheme, "raw={raw:?}");
        assert_eq!(parsed.as_str(), expected_canonical, "raw={raw:?}");
        // Canonical string must round-trip identically.
        assert_eq!(
            BookIdentifier::parse(parsed.as_str()).unwrap(),
            parsed,
            "round-trip failed for {expected_canonical:?}"
        );
    }

    let invalid_cases = [
        "",
        "978-0-306-40615-8",
        "ISBN:979-0-060-11561-5",
        "0-306-40615-3",
        "0378-5956",
        "03785955",
        "M-060-11561-6",
        "0A9-2002-12B4A105-8",
        "B159-D8FA-0124-0000-A",
        "0000-0001-8947-0000-8-0000-0000-A",
        "10.12/abc",
        "CN 99-0001",
        "CN 11-0000",
        "2023123456",
        "1011200123",
        "123456789",
        "LCCN:2001-0",
        "urn:nbn:d:123",
    ];

    for bad in invalid_cases {
        assert_eq!(
            BookIdentifier::parse(bad).unwrap_err(),
            BookDomainError::InvalidIdentifier,
            "expected {bad:?} to be rejected"
        );
    }
}

#[test]
fn book_metadata_entity_tracks_dirty_fields_and_round_trips() {
    use shiro_domain::library::book::entity::{BookMetadata, BookMetadataFields};

    let book_id = BookId::new();
    let mut meta = BookMetadata::empty(book_id.clone());
    assert_eq!(meta.book_id(), &book_id);
    assert_eq!(meta.status(), BookStatus::Active);
    assert_eq!(meta.subtitle(), None);
    assert!(meta.authors().is_empty());
    assert_eq!(meta.publisher(), None);
    assert_eq!(meta.published_date(), None);
    assert_eq!(meta.language(), None);
    assert_eq!(meta.description(), None);
    assert_eq!(meta.identifier(), None);
    assert_eq!(meta.deleted_at(), None);

    // Clearing already-empty fields is rejected as unchanged for every field.
    assert_eq!(
        meta.clear_subtitle().unwrap_err(),
        BookDomainError::SubtitleUnchanged
    );
    assert_eq!(
        meta.clear_authors().unwrap_err(),
        BookDomainError::AuthorsUnchanged
    );
    assert_eq!(
        meta.clear_publisher().unwrap_err(),
        BookDomainError::PublisherUnchanged
    );
    assert_eq!(
        meta.clear_published_date().unwrap_err(),
        BookDomainError::PublishedDateUnchanged
    );
    assert_eq!(
        meta.clear_language().unwrap_err(),
        BookDomainError::LanguageUnchanged
    );
    assert_eq!(
        meta.clear_description().unwrap_err(),
        BookDomainError::DescriptionUnchanged
    );
    assert_eq!(
        meta.clear_identifier().unwrap_err(),
        BookDomainError::IdentifierUnchanged
    );

    // Restoring active metadata fails.
    assert_eq!(meta.restore().unwrap_err(), BookDomainError::NotDeleted);

    let sub_change = meta
        .change_subtitle(BookSubtitle::parse("Shadow of the Moon").unwrap())
        .unwrap();
    assert_eq!(
        sub_change.fields(),
        BookMetadataFields::SUBTITLE | BookMetadataFields::UPDATED_AT
    );

    let authors_change = meta
        .change_authors(vec![
            BookAuthor::parse("Fuyumi Ono").unwrap(),
            BookAuthor::parse("Akihiro Yamada").unwrap(),
        ])
        .unwrap();
    assert_eq!(
        authors_change.fields(),
        BookMetadataFields::AUTHORS | BookMetadataFields::UPDATED_AT
    );
    // Reordering authors is recognized as a distinct change.
    let reorder_change = meta
        .change_authors(vec![
            BookAuthor::parse("Akihiro Yamada").unwrap(),
            BookAuthor::parse("Fuyumi Ono").unwrap(),
        ])
        .unwrap();
    assert_eq!(
        reorder_change.fields(),
        BookMetadataFields::AUTHORS | BookMetadataFields::UPDATED_AT
    );

    let pub_change = meta
        .change_publisher(BookPublisher::parse("O'Reilly").unwrap())
        .unwrap();
    assert_eq!(
        pub_change.fields(),
        BookMetadataFields::PUBLISHER | BookMetadataFields::UPDATED_AT
    );

    let date_change = meta
        .change_published_date(BookPublishedDate::parse("2025-01-15").unwrap())
        .unwrap();
    assert_eq!(
        date_change.fields(),
        BookMetadataFields::PUBLISHED_DATE | BookMetadataFields::UPDATED_AT
    );

    let lang_change = meta
        .change_language(BookLanguage::parse("en-us").unwrap())
        .unwrap();
    assert_eq!(
        lang_change.fields(),
        BookMetadataFields::LANGUAGE | BookMetadataFields::UPDATED_AT
    );

    let desc_change = meta
        .change_description(BookDescription::parse("Systems handbook").unwrap())
        .unwrap();
    assert_eq!(
        desc_change.fields(),
        BookMetadataFields::DESCRIPTION | BookMetadataFields::UPDATED_AT
    );

    let id_change = meta
        .change_identifier(BookIdentifier::parse("978-0-306-40615-7").unwrap())
        .unwrap();
    assert_eq!(
        id_change.fields(),
        BookMetadataFields::IDENTIFIER | BookMetadataFields::UPDATED_AT
    );

    // Unchanged concrete value is rejected for every field.
    assert_eq!(
        meta.change_subtitle(BookSubtitle::parse("Shadow of the Moon").unwrap())
            .unwrap_err(),
        BookDomainError::SubtitleUnchanged
    );
    assert_eq!(
        meta.change_authors(vec![
            BookAuthor::parse("Akihiro Yamada").unwrap(),
            BookAuthor::parse("Fuyumi Ono").unwrap(),
        ])
        .unwrap_err(),
        BookDomainError::AuthorsUnchanged
    );
    assert_eq!(
        meta.change_authors(vec![]).unwrap_err(),
        BookDomainError::InvalidAuthor
    );
    assert_eq!(
        meta.change_authors(vec![
            BookAuthor::parse("Dup").unwrap(),
            BookAuthor::parse("Dup").unwrap(),
        ])
        .unwrap_err(),
        BookDomainError::InvalidAuthor
    );
    assert_eq!(
        meta.change_publisher(BookPublisher::parse("O'Reilly").unwrap())
            .unwrap_err(),
        BookDomainError::PublisherUnchanged
    );
    assert_eq!(
        meta.change_published_date(BookPublishedDate::parse("2025-01-15").unwrap())
            .unwrap_err(),
        BookDomainError::PublishedDateUnchanged
    );
    assert_eq!(
        meta.change_language(BookLanguage::parse("en-US").unwrap())
            .unwrap_err(),
        BookDomainError::LanguageUnchanged
    );
    assert_eq!(
        meta.change_description(BookDescription::parse("Systems handbook").unwrap())
            .unwrap_err(),
        BookDomainError::DescriptionUnchanged
    );
    assert_eq!(
        meta.change_identifier(BookIdentifier::parse("978-0-306-40615-7").unwrap())
            .unwrap_err(),
        BookDomainError::IdentifierUnchanged
    );

    // Clearing populated fields succeeds, tracks dirty fields, and rejects duplicate clear.
    let clear_sub = meta.clear_subtitle().unwrap();
    assert_eq!(
        clear_sub.fields(),
        BookMetadataFields::SUBTITLE | BookMetadataFields::UPDATED_AT
    );
    assert_eq!(meta.subtitle(), None);
    assert_eq!(
        meta.clear_subtitle().unwrap_err(),
        BookDomainError::SubtitleUnchanged
    );

    let clear_authors = meta.clear_authors().unwrap();
    assert_eq!(
        clear_authors.fields(),
        BookMetadataFields::AUTHORS | BookMetadataFields::UPDATED_AT
    );
    assert!(meta.authors().is_empty());
    assert_eq!(
        meta.clear_authors().unwrap_err(),
        BookDomainError::AuthorsUnchanged
    );

    let clear_pub = meta.clear_publisher().unwrap();
    assert_eq!(
        clear_pub.fields(),
        BookMetadataFields::PUBLISHER | BookMetadataFields::UPDATED_AT
    );
    assert_eq!(meta.publisher(), None);
    assert_eq!(
        meta.clear_publisher().unwrap_err(),
        BookDomainError::PublisherUnchanged
    );

    let clear_date = meta.clear_published_date().unwrap();
    assert_eq!(
        clear_date.fields(),
        BookMetadataFields::PUBLISHED_DATE | BookMetadataFields::UPDATED_AT
    );
    assert_eq!(meta.published_date(), None);
    assert_eq!(
        meta.clear_published_date().unwrap_err(),
        BookDomainError::PublishedDateUnchanged
    );

    let clear_lang = meta.clear_language().unwrap();
    assert_eq!(
        clear_lang.fields(),
        BookMetadataFields::LANGUAGE | BookMetadataFields::UPDATED_AT
    );
    assert_eq!(meta.language(), None);
    assert_eq!(
        meta.clear_language().unwrap_err(),
        BookDomainError::LanguageUnchanged
    );

    let clear_desc = meta.clear_description().unwrap();
    assert_eq!(
        clear_desc.fields(),
        BookMetadataFields::DESCRIPTION | BookMetadataFields::UPDATED_AT
    );
    assert_eq!(meta.description(), None);
    assert_eq!(
        meta.clear_description().unwrap_err(),
        BookDomainError::DescriptionUnchanged
    );

    let clear_id = meta.clear_identifier().unwrap();
    assert_eq!(
        clear_id.fields(),
        BookMetadataFields::IDENTIFIER | BookMetadataFields::UPDATED_AT
    );
    assert_eq!(meta.identifier(), None);
    assert_eq!(
        meta.clear_identifier().unwrap_err(),
        BookDomainError::IdentifierUnchanged
    );

    // Re-populate fields so reconstruct round-trip exercises mixed Some/None and multi-author state.
    let _ = meta
        .change_subtitle(BookSubtitle::parse("Part 1").unwrap())
        .unwrap();
    let _ = meta
        .change_authors(vec![
            BookAuthor::parse("Alice").unwrap(),
            BookAuthor::parse("Bob").unwrap(),
        ])
        .unwrap();
    let _ = meta
        .change_publisher(BookPublisher::parse("O'Reilly").unwrap())
        .unwrap();

    // Soft delete tracks DELETED_AT | UPDATED_AT and blocks mutations + clears + duplicate deletes.
    let delete_change = meta.mark_deleted().unwrap();
    assert_eq!(
        delete_change.fields(),
        BookMetadataFields::DELETED_AT | BookMetadataFields::UPDATED_AT
    );
    assert!(matches!(meta.status(), BookStatus::Deleted { .. }));
    assert_eq!(meta.mark_deleted().unwrap_err(), BookDomainError::AlreadyDeleted);
    assert_eq!(
        meta.change_subtitle(BookSubtitle::parse("Blocked").unwrap())
            .unwrap_err(),
        BookDomainError::AlreadyDeleted
    );
    assert_eq!(
        meta.change_authors(vec![BookAuthor::parse("Blocked").unwrap()])
            .unwrap_err(),
        BookDomainError::AlreadyDeleted
    );
    assert_eq!(
        meta.clear_subtitle().unwrap_err(),
        BookDomainError::AlreadyDeleted
    );
    assert_eq!(
        meta.clear_authors().unwrap_err(),
        BookDomainError::AlreadyDeleted
    );
    assert_eq!(
        meta.change_publisher(BookPublisher::parse("Blocked").unwrap())
            .unwrap_err(),
        BookDomainError::AlreadyDeleted
    );
    assert_eq!(
        meta.change_published_date(BookPublishedDate::parse("2025").unwrap())
            .unwrap_err(),
        BookDomainError::AlreadyDeleted
    );
    assert_eq!(
        meta.change_language(BookLanguage::parse("en").unwrap())
            .unwrap_err(),
        BookDomainError::AlreadyDeleted
    );
    assert_eq!(
        meta.change_description(BookDescription::parse("Blocked").unwrap())
            .unwrap_err(),
        BookDomainError::AlreadyDeleted
    );
    assert_eq!(
        meta.change_identifier(BookIdentifier::parse("978-0-306-40615-7").unwrap())
            .unwrap_err(),
        BookDomainError::AlreadyDeleted
    );
    assert_eq!(
        meta.clear_publisher().unwrap_err(),
        BookDomainError::AlreadyDeleted
    );
    assert_eq!(
        meta.clear_published_date().unwrap_err(),
        BookDomainError::AlreadyDeleted
    );
    assert_eq!(
        meta.clear_language().unwrap_err(),
        BookDomainError::AlreadyDeleted
    );
    assert_eq!(
        meta.clear_description().unwrap_err(),
        BookDomainError::AlreadyDeleted
    );
    assert_eq!(
        meta.clear_identifier().unwrap_err(),
        BookDomainError::AlreadyDeleted
    );

    // Restore clears deleted_at and tracks DELETED_AT | UPDATED_AT.
    let restore_change = meta.restore().unwrap();
    assert_eq!(
        restore_change.fields(),
        BookMetadataFields::DELETED_AT | BookMetadataFields::UPDATED_AT
    );
    assert_eq!(meta.status(), BookStatus::Active);
    assert_eq!(meta.deleted_at(), None);

    // Reconstruct round-trip preserves all 11 fields.
    let expected = meta.clone();
    let inner = meta.into_inner();
    let reconstructed = BookMetadata::reconstruct(
        inner.book_id,
        inner.subtitle,
        inner.authors,
        inner.publisher,
        inner.published_date,
        inner.language,
        inner.description,
        inner.identifier,
        inner.created_at,
        inner.updated_at,
        inner.deleted_at,
    );
    assert_eq!(reconstructed, expected);
}

#[test]
fn book_subtitle_and_author_value_objects_enforce_validation_and_uniqueness() {
    assert_eq!(
        BookSubtitle::parse("A Concise Guide").unwrap().as_str(),
        "A Concise Guide"
    );
    let max_256 = "a".repeat(256);
    assert_eq!(BookSubtitle::parse(max_256.clone()).unwrap().as_str(), max_256);
    assert_eq!(BookAuthor::parse(max_256.clone()).unwrap().as_str(), max_256);

    let too_long = "a".repeat(257);
    for bad in ["", " Leading", "Trailing ", "Line\nBreak", "Null\0Char", &too_long] {
        assert_eq!(
            BookSubtitle::parse(bad).unwrap_err(),
            BookDomainError::InvalidSubtitle
        );
        assert_eq!(
            BookAuthor::parse(bad).unwrap_err(),
            BookDomainError::InvalidAuthor
        );
    }

    let ordered = BookAuthor::parse_list(vec!["Alice".into(), "Bob".into()]).unwrap();
    assert_eq!(
        ordered.iter().map(BookAuthor::as_str).collect::<Vec<_>>(),
        ["Alice", "Bob"]
    );

    assert_eq!(
        BookAuthor::parse_list(vec![]).unwrap_err(),
        BookDomainError::InvalidAuthor
    );
    assert_eq!(
        BookAuthor::parse_list(vec!["Alice".into(), "Alice".into()]).unwrap_err(),
        BookDomainError::InvalidAuthor
    );
    assert_eq!(
        BookAuthor::parse_list(vec!["Alice".into(), " ".into()]).unwrap_err(),
        BookDomainError::InvalidAuthor
    );
    let too_many: Vec<String> = (0..=MAX_BOOK_AUTHORS)
        .map(|index| format!("Author {index}"))
        .collect();
    assert_eq!(
        BookAuthor::parse_list(too_many).unwrap_err(),
        BookDomainError::InvalidAuthor
    );
}
