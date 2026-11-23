mod support;
use DeleteStrategy::*;
use SampleFile::Epub;
use TagError::*;
use support::*;

// 验证创建标签时拒绝同级重名，并保持标签树稳定。
#[tokio::test]
async fn creation_rejects_a_name_already_used_by_a_sibling() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    assert!(app.tag_tree(&reader).await.is_empty());
    let root = app.add_tag(&reader, "分类2026", None, false, None).await;
    let child = app
        .add_tag(&reader, "评分", Some(&root.id), true, Some("[0-9]+"))
        .await;
    let leaf = app
        .add_tag(&reader, "备注", Some(&child.id), true, None)
        .await;
    app.define_tag(&reader, "分类2026", None, false, None)
        .await
        .assert_tag_error(SiblingNameConflict);
    assert_eq!(
        app.tag_tree(&reader).await,
        [root.node(vec![child.node(vec![leaf.node(vec![])])])]
    );
}

// 验证标签名可以包含点号并作为翻译 key 原样保存和重命名。
#[tokio::test]
async fn dotted_tag_names_are_preserved_as_translation_keys() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    let tag = app
        .add_tag(&reader, "tag.genre.fiction", None, false, None)
        .await;
    assert_eq!(app.tag_tree(&reader).await, [tag.node(vec![])]);

    app.rename_tag(&reader, &tag.id, "tag.genre.nonfiction")
        .await
        .assert_empty(204);
    let tree = app.tag_tree(&reader).await;
    let renamed = TagDefinitionData::find(&tree, &tag.id).unwrap();
    assert_eq!(renamed.name, "tag.genre.nonfiction");
}

// 验证创建和重命名标签时拒绝非法名称，重命名为原名称也报错且标签树不变。
#[tokio::test]
async fn invalid_names_are_rejected_on_creation_and_rename_without_changing_the_tree() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    let tag = app.add_tag(&reader, "合法名称", None, false, None).await;
    for (name, error) in [
        ("", MissingName),
        (" leading", InvalidName),
        ("trailing ", InvalidName),
        ("two words", InvalidName),
        ("a\tb", InvalidName),
        ("a\nb", InvalidName),
        ("a\u{a0}b", InvalidName),
        ("a\u{200b}b", InvalidName),
        ("a\0b", InvalidName),
        ("under_score", InvalidName),
        ("with-dash", InvalidName),
        ("标点！", InvalidName),
        ("emoji😀", InvalidName),
        ("a/b", InvalidName),
    ] {
        app.define_tag(&reader, name, None, false, None)
            .await
            .assert_tag_error(error);
        app.rename_tag(&reader, &tag.id, name)
            .await
            .assert_tag_error(error);
    }
    app.rename_tag(&reader, &tag.id, &tag.name)
        .await
        .assert_tag_error(UnchangedName);
    assert_eq!(app.tag_tree(&reader).await, [tag.node(vec![])]);
}

// 验证创建标签时拒绝非法或不存在的父标签、非法正则及无值标签的正则规则。
#[tokio::test]
async fn creation_rejects_missing_parents_and_invalid_value_rules() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    app.define_tag(&reader, "Child", Some(UNKNOWN_ID), false, None)
        .await
        .assert_tag_error(ParentNotFound);
    app.define_tag(&reader, "Child", Some(INVALID_ID), false, None)
        .await
        .assert_tag_error(InvalidId);
    for regex in ["[", "(", "a)\\z|(?:b"] {
        app.define_tag(&reader, "BadRegex", None, true, Some(regex))
            .await
            .assert_tag_error(InvalidRegex);
    }
    app.define_tag_with_color(&reader, "BadColor", None, false, None, Some("#12GG56"))
        .await
        .assert_tag_error(InvalidColor);
    for regex in ["", "[0-9]+"] {
        app.define_tag(&reader, "NoValues", None, false, Some(regex))
            .await
            .assert_tag_error(RegexNotAllowed);
    }
    assert!(app.tag_tree(&reader).await.is_empty());
    let root = app.add_tag(&reader, "Root", None, false, None).await;
    app.delete_tag(&reader, &root.id, Cascade)
        .await
        .assert_empty(204);
    app.define_tag(&reader, "Child", Some(&root.id), false, None)
        .await
        .assert_tag_error(ParentNotFound);
}

// 验证标签重命名保留 ID、父子关系和值规则。
#[tokio::test]
async fn rename_preserves_identity_parent_children_and_value_rules() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    let root = app.add_tag(&reader, "Root", None, false, None).await;
    let tag = app
        .add_tag(&reader, "Rating", Some(&root.id), true, Some("[0-9]+"))
        .await;
    let child = app
        .add_tag(&reader, "Child", Some(&tag.id), false, None)
        .await;
    app.rename_tag(&reader, &tag.id, "Score")
        .await
        .assert_empty(204);
    let tree = app.tag_tree(&reader).await;
    let renamed = TagDefinitionData::find(&tree, &tag.id).unwrap();
    assert_eq!(renamed.created_at, tag.created_at);
    assert!(renamed.updated_at > tag.updated_at);
    let expected = vec![root.node(vec![
                tag.renamed("Score")
                    .modified_at(renamed.updated_at)
                    .node(vec![child.node(vec![])]),
            ])];
    assert_eq!(tree, expected);
}

// 验证重命名不能与同级标签重名，且冲突不会改变标签树。
#[tokio::test]
async fn rename_rejects_a_name_already_used_by_a_sibling() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    let first = app.add_tag(&reader, "First", None, false, None).await;
    let second = app.add_tag(&reader, "Second", None, false, None).await;
    let tree = app.tag_tree(&reader).await;

    app.rename_tag(&reader, &first.id, &second.name)
        .await
        .assert_tag_error(SiblingNameConflict);
    assert_eq!(app.tag_tree(&reader).await, tree);
}

// 验证创建必须提供颜色，且颜色 PATCH 校验格式并持久化更新。
#[tokio::test]
async fn colors_are_required_and_can_be_changed_using_rgb_hex_values() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    app.define_tag_with_color(&reader, "MissingColor", None, false, None, None)
        .await
        .assert_status(422);
    let changed_color = app
        .add_tag(&reader, "ChangedColor", None, false, None)
        .await;
    let explicit_color = app
        .add_tag_with_color(&reader, "ExplicitColor", None, false, None, Some("#a1B2c3"))
        .await;
    assert_eq!(explicit_color.color, "#a1B2c3");

    for color in [
        "",
        "000000",
        "#12345",
        "#1234567",
        "#12345678",
        "#GG0000",
        " #FFFFFF",
        "#ＦＦＦＦＦＦ",
    ] {
        app.change_tag_color(&reader, &changed_color.id, color)
            .await
            .assert_tag_error(InvalidColor);
    }
    assert_eq!(
        app.tag_tree(&reader).await,
        [changed_color.node(vec![]), explicit_color.node(vec![])]
    );

    app.change_tag_color(&reader, &changed_color.id, "#33FF00")
        .await
        .assert_empty(204);
    app.change_tag_color(&reader, &changed_color.id, "#33FF00")
        .await
        .assert_tag_error(UnchangedColor);
    let tree = app.tag_tree(&reader).await;
    let changed = TagDefinitionData::find(&tree, &changed_color.id).unwrap();
    assert_eq!(changed.color, "#33FF00");
    assert!(changed.updated_at > changed_color.updated_at);
    assert_eq!(
        tree,
        [
            changed_color
                .colored("#33FF00")
                .modified_at(changed.updated_at)
                .node(vec![]),
            explicit_color.node(vec![]),
        ]
    );
}

// 验证移动标签可更换父级或移至根层，同时保留子树、值规则和书籍绑定。
#[tokio::test]
async fn moving_a_tag_preserves_its_subtree_rules_and_book_bindings() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    app.upload(&reader, Epub).await;
    let book = app.add_book(&reader, "Book", Epub).await;
    let first_root = app.add_tag(&reader, "FirstRoot", None, false, None).await;
    let second_root = app.add_tag(&reader, "SecondRoot", None, false, None).await;
    let category = app
        .add_tag(
            &reader,
            "Category",
            Some(&first_root.id),
            true,
            Some("[A-Z]+"),
        )
        .await;
    let child = app
        .add_tag(&reader, "Child", Some(&category.id), false, None)
        .await;
    app.bind_tag(&reader, &book.data.id, &category.id, Some("FICTION"))
        .await
        .assert_empty(204);

    app.move_tag(&reader, &category.id, Some(&second_root.id))
        .await
        .assert_empty(204);
    app.move_tag(&reader, &category.id, Some(&second_root.id))
        .await
        .assert_tag_error(InvalidMove);
    let moved_tree = app.tag_tree(&reader).await;
    let moved_category = TagDefinitionData::find(&moved_tree, &category.id).unwrap();
    assert_eq!(moved_category.created_at, category.created_at);
    assert!(moved_category.updated_at > category.updated_at);
    assert_eq!(
        moved_tree,
        [
            first_root.node(vec![]),
            second_root.node(vec![
                category
                    .under(Some(&second_root.id))
                    .modified_at(moved_category.updated_at)
                    .node(vec![child.node(vec![])])
            ]),
        ]
    );

    app.move_tag(&reader, &category.id, None)
        .await
        .assert_empty(204);
    let root_tree = app.tag_tree(&reader).await;
    let moved_to_root = TagDefinitionData::find(&root_tree, &category.id).unwrap();
    assert_eq!(moved_to_root.created_at, category.created_at);
    assert!(moved_to_root.updated_at > moved_category.updated_at);
    assert_eq!(
        root_tree,
        [
            category
                .under(None)
                .modified_at(moved_to_root.updated_at)
                .node(vec![child.node(vec![])]),
            first_root.node(vec![]),
            second_root.node(vec![]),
        ]
    );
    app.assert_book_tags(&reader, &book.data.id, &[category.binding(Some("FICTION"))])
        .await;
}

// 验证移动拒绝不存在的目标、自挂载和移入后代，并且失败时不改变标签树。
#[tokio::test]
async fn moving_a_tag_rejects_missing_targets_and_cyclic_hierarchies() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    let root = app.add_tag(&reader, "Root", None, false, None).await;
    let branch = app
        .add_tag(&reader, "Branch", Some(&root.id), false, None)
        .await;
    let leaf = app
        .add_tag(&reader, "Leaf", Some(&branch.id), false, None)
        .await;

    for (id, error) in [(INVALID_ID, InvalidId), (UNKNOWN_ID, NotFound)] {
        app.move_tag(&reader, id, None)
            .await
            .assert_tag_error(error);
    }
    app.move_tag(&reader, &branch.id, Some(INVALID_ID))
        .await
        .assert_tag_error(InvalidId);
    app.move_tag(&reader, &branch.id, Some(UNKNOWN_ID))
        .await
        .assert_tag_error(ParentNotFound);
    app.move_tag(&reader, &branch.id, Some(&branch.id))
        .await
        .assert_tag_error(InvalidMove);
    app.move_tag(&reader, &root.id, Some(&leaf.id))
        .await
        .assert_tag_error(InvalidMove);
    assert_eq!(
        app.tag_tree(&reader).await,
        [root.node(vec![branch.node(vec![leaf.node(vec![])])])]
    );
}

// 验证移动标签后不能与目标父级下的标签重名，且冲突不会改变标签树。
#[tokio::test]
async fn moving_a_tag_rejects_a_name_already_used_at_the_destination() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    let moving = app.add_tag(&reader, "Moving", None, false, None).await;
    let destination = app.add_tag(&reader, "Destination", None, false, None).await;
    app.add_tag(&reader, "Moving", Some(&destination.id), false, None)
        .await;
    let tree = app.tag_tree(&reader).await;

    app.move_tag(&reader, &moving.id, Some(&destination.id))
        .await
        .assert_tag_error(SiblingNameConflict);
    assert_eq!(app.tag_tree(&reader).await, tree);
}

// 验证有值标签必须绑定完整匹配正则的非空字符串，无值标签拒绝任何值。
#[tokio::test]
async fn bindings_require_a_nonempty_string_matching_the_entire_regex() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    app.upload(&reader, Epub).await;
    let book = app.add_book(&reader, "Book", Epub).await;
    let tag = app
        .add_tag(&reader, "Rating", None, true, Some("[0-9]+"))
        .await;
    for value in [None, Some("")] {
        app.bind_tag(&reader, &book.data.id, &tag.id, value)
            .await
            .assert_tag_error(ValueRequired);
    }
    for value in ["abc", "12x", "x12", "12\n", " 12", " "] {
        app.bind_tag(&reader, &book.data.id, &tag.id, Some(value))
            .await
            .assert_tag_error(RegexMismatch);
    }
    app.bind_tag(&reader, &book.data.id, &tag.id, Some("12"))
        .await
        .assert_empty(204);
    let plain = app.add_tag(&reader, "Favorite", None, false, None).await;
    for value in ["", "anything"] {
        app.bind_tag(&reader, &book.data.id, &plain.id, Some(value))
            .await
            .assert_tag_error(ValueNotAllowed);
    }
    app.bind_tag(&reader, &book.data.id, &plain.id, None)
        .await
        .assert_empty(204);
    app.bind_tag(&reader, &book.data.id, &tag.id, Some("invalid"))
        .await
        .assert_tag_error(RegexMismatch);
    app.assert_book_tags(
        &reader,
        &book.data.id,
        &[tag.binding(Some("12")), plain.binding(None)],
    )
    .await;
    app.unbind_tag(&reader, &book.data.id, &plain.id)
        .await
        .assert_empty(204);
    app.assert_book_tags(&reader, &book.data.id, &[tag.binding(Some("12"))])
        .await;
}

// 验证加载标签定义后，无正则或空正则允许任意非空值，扩展模式和分支正则仍要求完整匹配。
#[tokio::test]
async fn optional_and_extended_regexes_work_after_loading_the_definition() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    app.upload(&reader, Epub).await;
    let book = app.add_book(&reader, "Book", Epub).await;
    for (name, regex) in [("自由值", None), ("自由值空正则", Some(""))] {
        let tag = app.add_tag(&reader, name, None, true, regex).await;
        app.bind_tag(&reader, &book.data.id, &tag.id, Some("任意内容 !\n"))
            .await
            .assert_empty(204);
        app.bind_tag(&reader, &book.data.id, &tag.id, None)
            .await
            .assert_tag_error(ValueRequired);
    }
    for (name, regex) in [
        ("扩展值", "(?x)[0-9]+ # trailing comment"),
        ("分支值", "1|22"),
    ] {
        let tag = app.add_tag(&reader, name, None, true, Some(regex)).await;
        app.bind_tag(&reader, &book.data.id, &tag.id, Some("22"))
            .await
            .assert_empty(204);
        app.bind_tag(&reader, &book.data.id, &tag.id, Some("22x"))
            .await
            .assert_tag_error(RegexMismatch);
    }
}

// 验证标签可独立绑定多本书、更新绑定值和重新绑定，且重复解绑具有幂等性。
#[tokio::test]
async fn a_tag_can_be_bound_to_multiple_books_and_unbinding_is_idempotent() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    app.upload(&reader, Epub).await;
    let first = app.add_book(&reader, "First", Epub).await;
    let second = app.add_book(&reader, "Second", Epub).await;
    let tag = app
        .add_tag(&reader, "Rating", None, true, Some("[0-9]+"))
        .await;
    app.assert_book_tags(&reader, &first.data.id, &[]).await;
    app.bind_tag(&reader, &first.data.id, &tag.id, Some("1"))
        .await
        .assert_empty(204);
    app.assert_book_tags(&reader, &first.data.id, &[tag.binding(Some("1"))])
        .await;
    app.bind_tag(&reader, &first.data.id, &tag.id, Some("2"))
        .await
        .assert_empty(204);
    app.assert_book_tags(&reader, &first.data.id, &[tag.binding(Some("2"))])
        .await;
    app.bind_tag(&reader, &second.data.id, &tag.id, Some("3"))
        .await
        .assert_empty(204);
    app.unbind_tag(&reader, &first.data.id, &tag.id)
        .await
        .assert_empty(204);
    app.assert_book_tags(&reader, &first.data.id, &[]).await;
    app.unbind_tag(&reader, &first.data.id, &tag.id)
        .await
        .assert_empty(204);
    app.assert_book_tags(&reader, &first.data.id, &[]).await;
    app.bind_tag(&reader, &first.data.id, &tag.id, Some("4"))
        .await
        .assert_empty(204);
    assert_eq!(app.tag_tree(&reader).await, [tag.node(vec![])]);
    app.assert_book_tags(&reader, &first.data.id, &[tag.binding(Some("4"))])
        .await;
    app.assert_book_tags(&reader, &second.data.id, &[tag.binding(Some("3"))])
        .await;
}

// 验证标签及绑定相关操作对非法或不存在的标签、书籍 ID 返回对应错误。
#[tokio::test]
async fn missing_or_invalid_targets_are_reported_for_all_mutations() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    app.upload(&reader, Epub).await;
    let book = app.add_book(&reader, "Book", Epub).await;
    let tag = app.add_tag(&reader, "Tag", None, false, None).await;
    for (id, error) in [(INVALID_ID, InvalidId), (UNKNOWN_ID, NotFound)] {
        app.rename_tag(&reader, id, "Renamed")
            .await
            .assert_tag_error(error);
        app.change_tag_color(&reader, id, "#FFFFFF")
            .await
            .assert_tag_error(error);
        for strategy in [Cascade, Promote] {
            app.delete_tag(&reader, id, strategy)
                .await
                .assert_tag_error(error);
        }
        app.bind_tag(&reader, &book.data.id, id, None)
            .await
            .assert_tag_error(error);
        app.unbind_tag(&reader, &book.data.id, id)
            .await
            .assert_tag_error(error);
    }
    for (id, error) in [
        (INVALID_ID, HttpError::InvalidBookId),
        (UNKNOWN_ID, HttpError::BookNotFound),
    ] {
        app.query_book_tags(&reader, id).await.assert_error(error);
        app.bind_tag(&reader, id, &tag.id, None)
            .await
            .assert_error(error);
        app.unbind_tag(&reader, id, &tag.id)
            .await
            .assert_error(error);
    }
    assert_eq!(app.tag_tree(&reader).await, [tag.node(vec![])]);
}

// 验证级联删除清除所选标签子树及其绑定，保留其他分支和书籍。
#[tokio::test]
async fn cascade_removes_only_the_selected_subtree_and_all_its_descendants() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    app.upload(&reader, Epub).await;
    let book = app.add_book(&reader, "Book", Epub).await;
    let root = app.add_tag(&reader, "Root", None, false, None).await;
    let branch = app
        .add_tag(&reader, "Branch", Some(&root.id), false, None)
        .await;
    let child = app
        .add_tag(&reader, "Child", Some(&branch.id), false, None)
        .await;
    let leaf = app
        .add_tag(&reader, "Leaf", Some(&child.id), false, None)
        .await;
    let sibling = app
        .add_tag(&reader, "Sibling", Some(&root.id), false, None)
        .await;
    for tag in [&branch, &child, &leaf, &sibling] {
        app.bind_tag(&reader, &book.data.id, &tag.id, None)
            .await
            .assert_empty(204);
    }
    app.delete_tag(&reader, &branch.id, Cascade)
        .await
        .assert_empty(204);
    app.assert_book_tags(&reader, &book.data.id, &[sibling.binding(None)])
        .await;
    assert_eq!(
        app.tag_tree(&reader).await,
        [root.node(vec![sibling.node(vec![])])]
    );
    for tag in [&branch, &child, &leaf] {
        app.bind_tag(&reader, &book.data.id, &tag.id, None)
            .await
            .assert_tag_error(NotFound);
    }
    app.delete_tag(&reader, &root.id, Cascade)
        .await
        .assert_empty(204);
    assert!(app.tag_tree(&reader).await.is_empty());
    assert_eq!(app.books(&reader).await, [book.data.clone()]);
    app.assert_book_tags(&reader, &book.data.id, &[]).await;
}

// 验证提升删除将所有直接子标签上移一级，保留更深层级、值规则和后代绑定。
#[tokio::test]
async fn promote_moves_all_direct_children_and_preserves_deeper_hierarchy() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    app.upload(&reader, Epub).await;
    let book = app.add_book(&reader, "Book", Epub).await;
    let root = app.add_tag(&reader, "Root", None, false, None).await;
    let branch = app
        .add_tag(&reader, "Branch", Some(&root.id), false, None)
        .await;
    let first = app
        .add_tag(&reader, "Achild", Some(&branch.id), true, Some("[0-9]+"))
        .await;
    let second = app
        .add_tag(&reader, "Bchild", Some(&branch.id), false, None)
        .await;
    let leaf = app
        .add_tag(&reader, "Leaf", Some(&first.id), false, None)
        .await;
    let sibling = app
        .add_tag(&reader, "Sibling", Some(&root.id), false, None)
        .await;
    for tag in [&root, &branch, &second, &leaf, &sibling] {
        app.bind_tag(&reader, &book.data.id, &tag.id, None)
            .await
            .assert_empty(204);
    }
    app.bind_tag(&reader, &book.data.id, &first.id, Some("7"))
        .await
        .assert_empty(204);
    app.delete_tag(&reader, &branch.id, Promote)
        .await
        .assert_empty(204);
    app.assert_book_tags(
        &reader,
        &book.data.id,
        &[
            root.binding(None),
            first.binding(Some("7")),
            second.binding(None),
            leaf.binding(None),
            sibling.binding(None),
        ],
    )
    .await;
    let promoted_tree = app.tag_tree(&reader).await;
    let promoted_first = TagDefinitionData::find(&promoted_tree, &first.id).unwrap();
    let promoted_second = TagDefinitionData::find(&promoted_tree, &second.id).unwrap();
    assert_eq!(promoted_first.created_at, first.created_at);
    assert_eq!(promoted_second.created_at, second.created_at);
    assert!(promoted_first.updated_at > first.updated_at);
    assert!(promoted_second.updated_at > second.updated_at);
    assert_eq!(
        promoted_tree,
        [root.node(vec![
            first
                .under(Some(&root.id))
                .modified_at(promoted_first.updated_at)
                .node(vec![leaf.node(vec![])]),
            second
                .under(Some(&root.id))
                .modified_at(promoted_second.updated_at)
                .node(vec![]),
            sibling.node(vec![]),
        ])]
    );
    app.delete_tag(&reader, &root.id, Promote)
        .await
        .assert_empty(204);
    let rootless_tree = app.tag_tree(&reader).await;
    let rootless_first = TagDefinitionData::find(&rootless_tree, &first.id).unwrap();
    let rootless_second = TagDefinitionData::find(&rootless_tree, &second.id).unwrap();
    let rootless_sibling = TagDefinitionData::find(&rootless_tree, &sibling.id).unwrap();
    assert!(rootless_first.updated_at > promoted_first.updated_at);
    assert!(rootless_second.updated_at > promoted_second.updated_at);
    assert!(rootless_sibling.updated_at > sibling.updated_at);
    assert_eq!(
        rootless_tree,
        [
            first
                .under(None)
                .modified_at(rootless_first.updated_at)
                .node(vec![leaf.node(vec![])]),
            second
                .under(None)
                .modified_at(rootless_second.updated_at)
                .node(vec![]),
            sibling
                .under(None)
                .modified_at(rootless_sibling.updated_at)
                .node(vec![]),
        ]
    );
    app.rename_tag(&reader, &first.id, "Arenamed")
        .await
        .assert_empty(204);
    app.assert_book_tags(
        &reader,
        &book.data.id,
        &[
            first.binding(Some("7")),
            second.binding(None),
            leaf.binding(None),
            sibling.binding(None),
        ],
    )
    .await;
}

// 验证提升删除不会制造同级重名，并在冲突时保留原标签树。
#[tokio::test]
async fn promote_delete_rejects_a_sibling_name_conflict() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    let root = app.add_tag(&reader, "Root", None, false, None).await;
    let branch = app
        .add_tag(&reader, "Branch", Some(&root.id), false, None)
        .await;
    app.add_tag(&reader, "Child", Some(&branch.id), false, None)
        .await;
    app.add_tag(&reader, "Child", Some(&root.id), false, None)
        .await;
    let tree = app.tag_tree(&reader).await;

    app.delete_tag(&reader, &branch.id, Promote)
        .await
        .assert_tag_error(SiblingNameConflict);
    assert_eq!(app.tag_tree(&reader).await, tree);
}

// 验证级联和提升两种策略均可删除叶子标签，且重复删除报错。
#[tokio::test]
async fn both_strategies_can_delete_a_leaf_and_reject_repeated_deletion() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    for strategy in [Cascade, Promote] {
        let leaf = app.add_tag(&reader, "Leaf", None, false, None).await;
        app.delete_tag(&reader, &leaf.id, strategy)
            .await
            .assert_empty(204);
        app.delete_tag(&reader, &leaf.id, strategy)
            .await
            .assert_tag_error(NotFound);
        assert!(app.tag_tree(&reader).await.is_empty());
    }
}

// 验证删除已绑定标签的书籍后保留标签定义及其他书籍的绑定。
#[tokio::test]
async fn deleting_a_bound_book_preserves_the_tag_definition_and_other_books() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    app.upload(&reader, Epub).await;
    let first = app.add_book(&reader, "First", Epub).await;
    let second = app.add_book(&reader, "Second", Epub).await;
    let tag = app.add_tag(&reader, "Tag", None, false, None).await;
    app.bind_tag(&reader, &first.data.id, &tag.id, None)
        .await
        .assert_empty(204);
    app.bind_tag(&reader, &second.data.id, &tag.id, None)
        .await
        .assert_empty(204);
    first.delete().await.assert_empty(204);
    app.query_book_tags(&reader, &first.data.id)
        .await
        .assert_error(HttpError::BookNotFound);
    app.assert_book_tags(&reader, &second.data.id, &[tag.binding(None)])
        .await;
    app.unbind_tag(&reader, &first.data.id, &tag.id)
        .await
        .assert_error(HttpError::BookNotFound);
    assert_eq!(app.tag_tree(&reader).await, [tag.node(vec![])]);
    assert_eq!(app.books(&reader).await, [second.data.clone()]);
    app.unbind_tag(&reader, &second.data.id, &tag.id)
        .await
        .assert_empty(204);
    app.assert_book_tags(&reader, &second.data.id, &[]).await;
}

// 验证重启后保留标签 ID、名称、层级、值规则及绑定，且已完成的删除不会恢复。
#[tokio::test]
async fn restart_preserves_tag_identity_hierarchy_rules_and_completed_deletions() {
    let directory = tempfile::tempdir().unwrap();
    let app = TestApp::at(directory.path()).await;
    let session = app.session("Reader").await;
    app.upload(&session, Epub).await;
    let book = app.add_book(&session, "Book", Epub).await.data;
    let root = app.add_tag(&session, "Root", None, false, None).await;
    let promoted = app
        .add_tag(&session, "Rating", Some(&root.id), true, Some("[0-9]+"))
        .await;
    let leaf = app
        .add_tag(&session, "Leaf", Some(&promoted.id), false, None)
        .await;
    let removed = app.add_tag(&session, "Removed", None, false, None).await;
    let removed_child = app
        .add_tag(&session, "RemovedChild", Some(&removed.id), false, None)
        .await;
    app.bind_tag(&session, &book.id, &promoted.id, Some("7"))
        .await
        .assert_empty(204);
    app.change_tag_color(&session, &promoted.id, "#BB44CC")
        .await
        .assert_empty(204);
    app.rename_tag(&session, &promoted.id, "Score")
        .await
        .assert_empty(204);
    app.delete_tag(&session, &root.id, Promote)
        .await
        .assert_empty(204);
    app.delete_tag(&session, &removed.id, Cascade)
        .await
        .assert_empty(204);
    let tree = app.tag_tree(&session).await;
    let persisted_promoted = TagDefinitionData::find(&tree, &promoted.id).unwrap();
    let expected_promoted = promoted
        .colored("#BB44CC")
        .renamed("Score")
        .under(None)
        .modified_at(persisted_promoted.updated_at);
    let deleted_ids = [root.id, removed.id, removed_child.id];
    drop(app);

    let app = TestApp::at(directory.path()).await;
    assert_eq!(
        app.tag_tree(&session).await,
        [expected_promoted.node(vec![leaf.node(vec![])])]
    );
    assert_eq!(app.books(&session).await, [book.clone()]);
    app.assert_book_tags(&session, &book.id, &[expected_promoted.binding(Some("7"))])
        .await;
    app.bind_tag(&session, &book.id, &expected_promoted.id, Some("bad"))
        .await
        .assert_tag_error(RegexMismatch);
    app.bind_tag(&session, &book.id, &expected_promoted.id, Some("8"))
        .await
        .assert_empty(204);
    for id in deleted_ids {
        app.rename_tag(&session, &id, "Missing")
            .await
            .assert_tag_error(NotFound);
    }
    app.rename_tag(&session, &expected_promoted.id, "AfterRestart")
        .await
        .assert_empty(204);
    let tree = app.tag_tree(&session).await;
    let renamed = TagDefinitionData::find(&tree, &expected_promoted.id).unwrap();
    assert_eq!(renamed.created_at, expected_promoted.created_at);
    assert!(renamed.updated_at > expected_promoted.updated_at);
    assert_eq!(
        tree,
        [expected_promoted
            .renamed("AfterRestart")
            .modified_at(renamed.updated_at)
            .node(vec![leaf.node(vec![])])]
    );
    app.assert_book_tags(&session, &book.id, &[expected_promoted.binding(Some("8"))])
        .await;
}

// 验证删除标签时缺失或非法的 strategy 查询参数返回 400 且不修改标签树。
#[tokio::test]
async fn delete_tag_rejects_missing_or_invalid_strategy_query() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    let tag = app.add_tag(&reader, "KeepMe", None, false, None).await;

    for raw_query in ["", "?strategy=INVALID"] {
        app.delete_tag_raw(&reader, &tag.id, raw_query)
            .await
            .assert_tag_error(InvalidDeleteStrategy);
    }
    assert_eq!(app.tag_tree(&reader).await, [tag.node(vec![])]);
}

// 验证新用户预置的默认标签包含语言和 ISBN 正则规则，并通过 HTTP 绑定接口生效。
#[tokio::test]
async fn default_tags_enforce_language_and_isbn_rules() {
    let app = TestApp::new().await;
    app.register("DefaultsUser").await.assert_ok();
    let reader = app.login("DefaultsUser", PASSWORD).await;
    app.upload(&reader, Epub).await;
    let book = app.add_book(&reader, "TaggedDefaults", Epub).await;

    let tree = app.tag_tree(&reader).await;
    let mut names: Vec<&str> = tree.iter().map(|node| node.name.as_str()).collect();
    names.sort();
    assert_eq!(
        names,
        [
            "tag.author",
            "tag.edition",
            "tag.issnIsbn",
            "tag.language",
            "tag.publisher",
        ]
    );

    let language_id = tree
        .iter()
        .find(|node| node.name == "tag.language")
        .unwrap()
        .id
        .clone();
    let isbn_id = tree
        .iter()
        .find(|node| node.name == "tag.issnIsbn")
        .unwrap()
        .id
        .clone();

    app.bind_tag(&reader, &book.data.id, &language_id, Some("zh-Hans"))
        .await
        .assert_empty(204);
    app.bind_tag(&reader, &book.data.id, &language_id, Some("en"))
        .await
        .assert_empty(204);
    app.bind_tag(&reader, &book.data.id, &language_id, Some("invalid-lang"))
        .await
        .assert_tag_error(RegexMismatch);

    app.bind_tag(&reader, &book.data.id, &isbn_id, Some("978-0-306-40615-7"))
        .await
        .assert_empty(204);
    app.bind_tag(&reader, &book.data.id, &isbn_id, Some("1234-567X"))
        .await
        .assert_empty(204);
    app.bind_tag(&reader, &book.data.id, &isbn_id, Some("not-isbn"))
        .await
        .assert_tag_error(RegexMismatch);
}
