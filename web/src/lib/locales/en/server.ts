import type { zhServer } from "../zh/server"

export const enServer: Record<keyof typeof zhServer, string> = {
  "identity.user.not_found": "User does not exist.",
  "identity.user.invalid_id": "Invalid user ID.",
  "identity.user.invalid_password_hash": "Invalid password hash.",
  "identity.user.name_conflict": "That username is already in use.",
  "identity.user.name_unchanged": "The username has not changed.",
  "identity.user.password_hash_unchanged": "The password has not changed.",
  "identity.user.version_conflict":
    "User data has changed. Please refresh and try again.",
  "library.book.version_conflict":
    "Book data has changed. Please review the latest data and try again.",
  "library.tag.invalid_id": "Invalid tag ID.",
  "library.tag.missing_name": "Please enter a tag name.",
  "library.tag.invalid_name_format":
    "Tag names can only contain letters, numbers, and periods.",
  "library.tag.name_unchanged": "The tag name has not changed.",
  "library.tag.sibling_name_conflict":
    "Tags at the same level must have different names.",
  "library.tag.invalid_value_regex": "Invalid tag value pattern.",
  "library.tag.value_regex_not_allowed":
    "Tags without values cannot have a value pattern.",
  "library.tag.value_required": "Please enter a tag value.",
  "library.tag.value_not_allowed": "This tag does not allow a value.",
  "library.tag.value_regex_mismatch":
    "The tag value does not match the required pattern.",
  "library.tag.not_found": "Tag not found.",
  "library.tag.parent_not_found":
    "Parent tag not found. Please refresh and try again.",
  "library.tag.invalid_move":
    "A tag cannot be moved under itself or one of its descendants.",
  "library.tag.version_conflict":
    "Tag data has changed. Please refresh and try again.",
  "library.tag.invalid_color": "Invalid tag color. Use the #RRGGBB format.",
  "library.tag.color_unchanged": "The tag color has not changed.",
  "library.tag.invalid_delete_strategy": "Invalid tag deletion strategy.",
  "identity.authentication.invalid_credentials":
    "Incorrect username or password.",
  "identity.authentication.invalid_token":
    "Your session has expired. Please sign in again.",
  "identity.authentication.invalid_token_type":
    "Your session has expired. Please sign in again.",
  "identity.authentication.missing_bearer_token": "Please sign in first.",
  "identity.authentication.user_not_found":
    "User not found. Please sign in again.",
  "identity.user.missing_user_name": "Please enter a username.",
  "identity.user.invalid_user_name_format": "Invalid username format.",
  "common.payload_too_large": "The upload is too large.",
  "common.internal_server_error":
    "Internal server error. Please try again later.",
  "library.book.invalid_id": "Invalid book ID.",
  "library.book.missing_title": "Please enter a book title.",
  "library.book.invalid_title_format": "Invalid book title format.",
  "library.book.invalid_hash_format": "Invalid book hash format.",
  "library.book.invalid_file": "Invalid book file.",
  "library.book.not_found": "Book not found.",
  "library.book.title_conflict": "A book with this title already exists.",
  "library.book.title_unchanged": "The book title has not changed.",
  "library.book.file.upload.conflict":
    "File upload conflict. Please try again.",
  "library.book.file.already_exists": "The book file already exists.",
  "library.book.file.invalid_hash_format": "Invalid file hash format.",
  "library.book.file.hash_mismatch":
    "The file hash does not match the supplied hash.",
  "library.book.file.not_found": "Book file not found.",
  "library.book.file.unsupported": "This book file format is not supported.",
  "library.book.file.upload.duplicate_hash":
    "The upload contains duplicate hash fields.",
  "library.book.file.upload.missing_hash":
    "The upload is missing the file hash.",
  "library.book.file.upload.missing_file": "Please select a file to upload.",
}
