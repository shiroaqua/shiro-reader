mod support;
use support::*;
use DeleteStrategy::*;
use SampleFile::Epub;
use TagError::*;

// 验证标签定义形成稳定的树形结构，且同名标签拥有不同 ID。
#[tokio::test]
async fn definitions_form_a_tree_and_keep_distinct_ids_even_with_the_same_name() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    assert!(app.tag_tree(&reader).await.is_empty());
    let root = app.add_tag(&reader, "分类2026", None, false, None).await;
    let child = app.add_tag(&reader, "评分", Some(&root.id), true, Some("[0-9]+")).await;
    let leaf = app.add_tag(&reader, "备注", Some(&child.id), true, None).await;
    let duplicate = app.add_tag(&reader, "分类2026", None, false, None).await;
    assert_ne!(root.id, duplicate.id);
    let tree = app.tag_tree(&reader).await;
    assert_eq!(tree.len(), 2);
    assert!(tree.contains(&root.node(vec![child.node(vec![leaf.node(vec![])])])));
    assert!(tree.contains(&duplicate.node(vec![])));
    assert_eq!(app.tag_tree(&reader).await, tree);
}

// 验证创建和重命名标签时拒绝非法名称，重命名为原名称也报错且标签树不变。
#[tokio::test]
async fn invalid_names_are_rejected_on_creation_and_rename_without_changing_the_tree() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    let tag = app.add_tag(&reader, "合法名称", None, false, None).await;
    for (name, error) in [
        ("", MissingName), (" leading", InvalidName), ("trailing ", InvalidName),
        ("two words", InvalidName), ("a\tb", InvalidName), ("a\nb", InvalidName),
        ("a\u{a0}b", InvalidName), ("a\u{200b}b", InvalidName), ("a\0b", InvalidName),
        ("under_score", InvalidName), ("with-dash", InvalidName), ("标点！", InvalidName),
        ("emoji😀", InvalidName), ("a/b", InvalidName),
    ] {
        app.define_tag(&reader, name, None, false, None).await.assert_tag_error(error);
        app.rename_tag(&reader, &tag.id, name).await.assert_tag_error(error);
    }
    app.rename_tag(&reader, &tag.id, &tag.name).await.assert_tag_error(UnchangedName);
    assert_eq!(app.tag_tree(&reader).await, [tag.node(vec![])]);
}

// 验证创建标签时拒绝非法或不存在的父标签、非法正则及无值标签的正则规则。
#[tokio::test]
async fn creation_rejects_missing_parents_and_invalid_value_rules() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    app.define_tag(&reader, "Child", Some(UNKNOWN_ID), false, None).await.assert_tag_error(ParentNotFound);
    app.define_tag(&reader, "Child", Some(INVALID_ID), false, None).await.assert_tag_error(InvalidId);
    for regex in ["[", "(", "a)\\z|(?:b"] {
        app.define_tag(&reader, "BadRegex", None, true, Some(regex)).await.assert_tag_error(InvalidRegex);
    }
    for regex in ["", "[0-9]+"] {
        app.define_tag(&reader, "NoValues", None, false, Some(regex)).await.assert_tag_error(RegexNotAllowed);
    }
    assert!(app.tag_tree(&reader).await.is_empty());
    let root = app.add_tag(&reader, "Root", None, false, None).await;
    app.delete_tag(&reader, &root.id, Cascade).await.assert_empty(204);
    app.define_tag(&reader, "Child", Some(&root.id), false, None).await.assert_tag_error(ParentNotFound);
}

// 验证标签重命名保留 ID、父子关系和值规则。
#[tokio::test]
async fn rename_preserves_identity_parent_children_and_value_rules() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    let root = app.add_tag(&reader, "Root", None, false, None).await;
    let tag = app.add_tag(&reader, "Rating", Some(&root.id), true, Some("[0-9]+")).await;
    let child = app.add_tag(&reader, "Child", Some(&tag.id), false, None).await;
    app.rename_tag(&reader, &tag.id, "Score").await.assert_empty(204);
    let expected = vec![root.node(vec![tag.renamed("Score").node(vec![child.node(vec![])])])];
    assert_eq!(app.tag_tree(&reader).await, expected);
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
    let category = app.add_tag(&reader, "Category", Some(&first_root.id), true, Some("[A-Z]+")).await;
    let child = app.add_tag(&reader, "Child", Some(&category.id), false, None).await;
    app.bind_tag(&reader, &book.data.id, &category.id, Some("FICTION")).await.assert_empty(204);

    app.move_tag(&reader, &category.id, Some(&second_root.id)).await.assert_empty(204);
    app.move_tag(&reader, &category.id, Some(&second_root.id)).await.assert_tag_error(InvalidMove);
    assert_eq!(app.tag_tree(&reader).await, [
        first_root.node(vec![]),
        second_root.node(vec![category.under(Some(&second_root.id)).node(vec![child.node(vec![])])]),
    ]);

    app.move_tag(&reader, &category.id, None).await.assert_empty(204);
    assert_eq!(app.tag_tree(&reader).await, [
        category.under(None).node(vec![child.node(vec![])]),
        first_root.node(vec![]),
        second_root.node(vec![]),
    ]);
    app.assert_book_tags(&reader, &book.data.id, &[category.binding(Some("FICTION"))]).await;
}

// 验证移动拒绝不存在的目标、自挂载和移入后代，并且失败时不改变标签树。
#[tokio::test]
async fn moving_a_tag_rejects_missing_targets_and_cyclic_hierarchies() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    let root = app.add_tag(&reader, "Root", None, false, None).await;
    let branch = app.add_tag(&reader, "Branch", Some(&root.id), false, None).await;
    let leaf = app.add_tag(&reader, "Leaf", Some(&branch.id), false, None).await;

    for (id, error) in [(INVALID_ID, InvalidId), (UNKNOWN_ID, NotFound)] {
        app.move_tag(&reader, id, None).await.assert_tag_error(error);
    }
    app.move_tag(&reader, &branch.id, Some(INVALID_ID)).await.assert_tag_error(InvalidId);
    app.move_tag(&reader, &branch.id, Some(UNKNOWN_ID)).await.assert_tag_error(ParentNotFound);
    app.move_tag(&reader, &branch.id, Some(&branch.id)).await.assert_tag_error(InvalidMove);
    app.move_tag(&reader, &root.id, Some(&leaf.id)).await.assert_tag_error(InvalidMove);
    assert_eq!(app.tag_tree(&reader).await, [root.node(vec![branch.node(vec![leaf.node(vec![])])])]);
}

// 验证有值标签必须绑定完整匹配正则的非空字符串，无值标签拒绝任何值。
#[tokio::test]
async fn bindings_require_a_nonempty_string_matching_the_entire_regex() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    app.upload(&reader, Epub).await;
    let book = app.add_book(&reader, "Book", Epub).await;
    let tag = app.add_tag(&reader, "Rating", None, true, Some("[0-9]+")).await;
    for value in [None, Some("")] {
        app.bind_tag(&reader, &book.data.id, &tag.id, value).await.assert_tag_error(ValueRequired);
    }
    for value in ["abc", "12x", "x12", "12\n", " 12", " "] {
        app.bind_tag(&reader, &book.data.id, &tag.id, Some(value)).await.assert_tag_error(RegexMismatch);
    }
    app.bind_tag(&reader, &book.data.id, &tag.id, Some("12")).await.assert_empty(204);
    let plain = app.add_tag(&reader, "Favorite", None, false, None).await;
    for value in ["", "anything"] {
        app.bind_tag(&reader, &book.data.id, &plain.id, Some(value)).await.assert_tag_error(ValueNotAllowed);
    }
    app.bind_tag(&reader, &book.data.id, &plain.id, None).await.assert_empty(204);
    app.bind_tag(&reader, &book.data.id, &tag.id, Some("invalid")).await.assert_tag_error(RegexMismatch);
    app.assert_book_tags(&reader, &book.data.id, &[tag.binding(Some("12")), plain.binding(None)]).await;
    app.unbind_tag(&reader, &book.data.id, &plain.id).await.assert_empty(204);
    app.assert_book_tags(&reader, &book.data.id, &[tag.binding(Some("12"))]).await;
}

// 验证加载标签定义后，无正则或空正则允许任意非空值，扩展模式和分支正则仍要求完整匹配。
#[tokio::test]
async fn optional_and_extended_regexes_work_after_loading_the_definition() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    app.upload(&reader, Epub).await;
    let book = app.add_book(&reader, "Book", Epub).await;
    for regex in [None, Some("")] {
        let tag = app.add_tag(&reader, "自由值", None, true, regex).await;
        app.bind_tag(&reader, &book.data.id, &tag.id, Some("任意内容 !\n")).await.assert_empty(204);
        app.bind_tag(&reader, &book.data.id, &tag.id, None).await.assert_tag_error(ValueRequired);
    }
    for regex in ["(?x)[0-9]+ # trailing comment", "1|22"] {
        let tag = app.add_tag(&reader, "限定值", None, true, Some(regex)).await;
        app.bind_tag(&reader, &book.data.id, &tag.id, Some("22")).await.assert_empty(204);
        app.bind_tag(&reader, &book.data.id, &tag.id, Some("22x")).await.assert_tag_error(RegexMismatch);
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
    let tag = app.add_tag(&reader, "Rating", None, true, Some("[0-9]+")).await;
    app.assert_book_tags(&reader, &first.data.id, &[]).await;
    app.bind_tag(&reader, &first.data.id, &tag.id, Some("1")).await.assert_empty(204);
    app.assert_book_tags(&reader, &first.data.id, &[tag.binding(Some("1"))]).await;
    app.bind_tag(&reader, &first.data.id, &tag.id, Some("2")).await.assert_empty(204);
    app.assert_book_tags(&reader, &first.data.id, &[tag.binding(Some("2"))]).await;
    app.bind_tag(&reader, &second.data.id, &tag.id, Some("3")).await.assert_empty(204);
    app.unbind_tag(&reader, &first.data.id, &tag.id).await.assert_empty(204);
    app.assert_book_tags(&reader, &first.data.id, &[]).await;
    app.unbind_tag(&reader, &first.data.id, &tag.id).await.assert_empty(204);
    app.assert_book_tags(&reader, &first.data.id, &[]).await;
    app.bind_tag(&reader, &first.data.id, &tag.id, Some("4")).await.assert_empty(204);
    assert_eq!(app.tag_tree(&reader).await, [tag.node(vec![])]);
    app.assert_book_tags(&reader, &first.data.id, &[tag.binding(Some("4"))]).await;
    app.assert_book_tags(&reader, &second.data.id, &[tag.binding(Some("3"))]).await;
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
        app.rename_tag(&reader, id, "Renamed").await.assert_tag_error(error);
        for strategy in [Cascade, Promote] {
            app.delete_tag(&reader, id, strategy).await.assert_tag_error(error);
        }
        app.bind_tag(&reader, &book.data.id, id, None).await.assert_tag_error(error);
        app.unbind_tag(&reader, &book.data.id, id).await.assert_tag_error(error);
    }
    for (id, error) in [(INVALID_ID, HttpError::InvalidBookId), (UNKNOWN_ID, HttpError::BookNotFound)] {
        app.query_book_tags(&reader, id).await.assert_error(error);
        app.bind_tag(&reader, id, &tag.id, None).await.assert_error(error);
        app.unbind_tag(&reader, id, &tag.id).await.assert_error(error);
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
    let branch = app.add_tag(&reader, "Branch", Some(&root.id), false, None).await;
    let child = app.add_tag(&reader, "Child", Some(&branch.id), false, None).await;
    let leaf = app.add_tag(&reader, "Leaf", Some(&child.id), false, None).await;
    let sibling = app.add_tag(&reader, "Sibling", Some(&root.id), false, None).await;
    for tag in [&branch, &child, &leaf, &sibling] {
        app.bind_tag(&reader, &book.data.id, &tag.id, None).await.assert_empty(204);
    }
    app.delete_tag(&reader, &branch.id, Cascade).await.assert_empty(204);
    app.assert_book_tags(&reader, &book.data.id, &[sibling.binding(None)]).await;
    assert_eq!(app.tag_tree(&reader).await, [root.node(vec![sibling.node(vec![])])]);
    for tag in [&branch, &child, &leaf] {
        app.bind_tag(&reader, &book.data.id, &tag.id, None).await.assert_tag_error(NotFound);
    }
    app.delete_tag(&reader, &root.id, Cascade).await.assert_empty(204);
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
    let branch = app.add_tag(&reader, "Branch", Some(&root.id), false, None).await;
    let first = app.add_tag(&reader, "Achild", Some(&branch.id), true, Some("[0-9]+")).await;
    let second = app.add_tag(&reader, "Bchild", Some(&branch.id), false, None).await;
    let leaf = app.add_tag(&reader, "Leaf", Some(&first.id), false, None).await;
    let sibling = app.add_tag(&reader, "Sibling", Some(&root.id), false, None).await;
    for tag in [&root, &branch, &second, &leaf, &sibling] {
        app.bind_tag(&reader, &book.data.id, &tag.id, None).await.assert_empty(204);
    }
    app.bind_tag(&reader, &book.data.id, &first.id, Some("7")).await.assert_empty(204);
    app.delete_tag(&reader, &branch.id, Promote).await.assert_empty(204);
    app.assert_book_tags(&reader, &book.data.id, &[
        root.binding(None), first.binding(Some("7")), second.binding(None),
        leaf.binding(None), sibling.binding(None),
    ]).await;
    assert_eq!(app.tag_tree(&reader).await, [root.node(vec![
        first.under(Some(&root.id)).node(vec![leaf.node(vec![])]),
        second.under(Some(&root.id)).node(vec![]), sibling.node(vec![]),
    ])]);
    app.delete_tag(&reader, &root.id, Promote).await.assert_empty(204);
    assert_eq!(app.tag_tree(&reader).await, [
        first.under(None).node(vec![leaf.node(vec![])]),
        second.under(None).node(vec![]), sibling.under(None).node(vec![]),
    ]);
    app.rename_tag(&reader, &first.id, "Arenamed").await.assert_empty(204);
    app.assert_book_tags(&reader, &book.data.id, &[
        first.binding(Some("7")), second.binding(None), leaf.binding(None), sibling.binding(None),
    ]).await;
}

// 验证级联和提升两种策略均可删除叶子标签，且重复删除报错。
#[tokio::test]
async fn both_strategies_can_delete_a_leaf_and_reject_repeated_deletion() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    for strategy in [Cascade, Promote] {
        let leaf = app.add_tag(&reader, "Leaf", None, false, None).await;
        app.delete_tag(&reader, &leaf.id, strategy).await.assert_empty(204);
        app.delete_tag(&reader, &leaf.id, strategy).await.assert_tag_error(NotFound);
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
    app.bind_tag(&reader, &first.data.id, &tag.id, None).await.assert_empty(204);
    app.bind_tag(&reader, &second.data.id, &tag.id, None).await.assert_empty(204);
    first.delete().await.assert_empty(204);
    app.query_book_tags(&reader, &first.data.id).await.assert_error(HttpError::BookNotFound);
    app.assert_book_tags(&reader, &second.data.id, &[tag.binding(None)]).await;
    app.unbind_tag(&reader, &first.data.id, &tag.id).await.assert_error(HttpError::BookNotFound);
    assert_eq!(app.tag_tree(&reader).await, [tag.node(vec![])]);
    assert_eq!(app.books(&reader).await, [second.data.clone()]);
    app.unbind_tag(&reader, &second.data.id, &tag.id).await.assert_empty(204);
    app.assert_book_tags(&reader, &second.data.id, &[]).await;
}

// 验证重启后保留标签 ID、名称、层级、值规则及绑定，且已完成的删除不会恢复。
#[tokio::test]
async fn restart_preserves_tag_identity_hierarchy_rules_and_completed_deletions() {
    let restart = Restart::new();
    restart.seed_tag_tree().await;
    restart.assert_tag_tree_restored().await;
}
