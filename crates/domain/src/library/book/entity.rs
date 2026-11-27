use blake3::Hash;
use entity_derive::Entity;
use jiff::Timestamp;

use crate::library::book::{
    errors::BookDomainError,
    value_objects::{
        BookAuthor, BookDescription, BookHash, BookId, BookIdentifier, BookLanguage,
        BookPublishedDate, BookPublisher, BookStatus, BookSubtitle, BookTitle,
    },
};

#[derive(Debug, Clone, PartialEq, Eq, Entity)]
pub struct Book {
    #[key]
    id: BookId,
    title: BookTitle,
    hash: BookHash,
    created_at: Timestamp,
    updated_at: Timestamp,
    deleted_at: Option<Timestamp>,
}

impl Book {
    pub fn new(id: BookId, title: BookTitle, hash: BookHash) -> Self {
        let created_at = Timestamp::now();
        Self {
            id,
            title,
            hash,
            created_at,
            updated_at: created_at,
            deleted_at: None,
        }
    }

    pub const fn reconstruct(
        id: BookId,
        title: BookTitle,
        hash: BookHash,
        created_at: Timestamp,
        updated_at: Timestamp,
        deleted_at: Option<Timestamp>,
    ) -> Self {
        Self {
            id,
            title,
            hash,
            created_at,
            updated_at,
            deleted_at,
        }
    }

    pub fn id(&self) -> &BookId {
        &self.id
    }

    pub fn title(&self) -> &BookTitle {
        &self.title
    }

    pub fn hash(&self) -> &Hash {
        self.hash.as_hash()
    }

    pub fn book_hash(&self) -> &BookHash {
        &self.hash
    }

    pub fn created_at(&self) -> Timestamp {
        self.created_at
    }

    pub fn updated_at(&self) -> Timestamp {
        self.updated_at
    }

    pub fn deleted_at(&self) -> Option<Timestamp> {
        self.deleted_at
    }

    pub fn status(&self) -> BookStatus {
        match self.deleted_at {
            Some(deleted_at) => BookStatus::Deleted { deleted_at },
            None => BookStatus::Active,
        }
    }

    pub fn mark_deleted(&mut self) -> Result<BookChange<'_>, BookDomainError> {
        if self.status() != BookStatus::Active {
            return Err(BookDomainError::AlreadyDeleted);
        }
        let now = Timestamp::now();
        self.deleted_at = Some(now);
        self.updated_at = now;
        Ok(BookChange::new(
            self,
            BookFields::DELETED_AT | BookFields::UPDATED_AT,
        ))
    }

    pub fn restore(&mut self, title: BookTitle) -> Result<BookChange<'_>, BookDomainError> {
        if self.status() == BookStatus::Active {
            return Err(BookDomainError::NotDeleted);
        }
        let mut fields = BookFields::DELETED_AT | BookFields::UPDATED_AT;
        if self.title != title {
            self.title = title;
            fields |= BookFields::TITLE;
        }
        self.deleted_at = None;
        self.updated_at = Timestamp::now();
        Ok(BookChange::new(self, fields))
    }

    pub fn rename(&mut self, new_title: BookTitle) -> Result<BookChange<'_>, BookDomainError> {
        if self.status() != BookStatus::Active {
            return Err(BookDomainError::AlreadyDeleted);
        }
        if self.title == new_title {
            return Err(BookDomainError::TitleUnchanged);
        }

        self.title = new_title;
        self.updated_at = Timestamp::now();
        Ok(BookChange::new(
            self,
            BookFields::TITLE | BookFields::UPDATED_AT,
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Entity)]
pub struct BookMetadata {
    #[key]
    book_id: BookId,
    subtitle: Option<BookSubtitle>,
    authors: Vec<BookAuthor>,
    publisher: Option<BookPublisher>,
    published_date: Option<BookPublishedDate>,
    language: Option<BookLanguage>,
    description: Option<BookDescription>,
    identifier: Option<BookIdentifier>,
    created_at: Timestamp,
    updated_at: Timestamp,
    deleted_at: Option<Timestamp>,
}

impl BookMetadata {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        book_id: BookId,
        subtitle: Option<BookSubtitle>,
        authors: Vec<BookAuthor>,
        publisher: Option<BookPublisher>,
        published_date: Option<BookPublishedDate>,
        language: Option<BookLanguage>,
        description: Option<BookDescription>,
        identifier: Option<BookIdentifier>,
    ) -> Self {
        let created_at = Timestamp::now();
        Self {
            book_id,
            subtitle,
            authors,
            publisher,
            published_date,
            language,
            description,
            identifier,
            created_at,
            updated_at: created_at,
            deleted_at: None,
        }
    }

    pub fn empty(book_id: BookId) -> Self {
        Self::new(book_id, None, Vec::new(), None, None, None, None, None)
    }

    #[allow(clippy::too_many_arguments)]
    pub const fn reconstruct(
        book_id: BookId,
        subtitle: Option<BookSubtitle>,
        authors: Vec<BookAuthor>,
        publisher: Option<BookPublisher>,
        published_date: Option<BookPublishedDate>,
        language: Option<BookLanguage>,
        description: Option<BookDescription>,
        identifier: Option<BookIdentifier>,
        created_at: Timestamp,
        updated_at: Timestamp,
        deleted_at: Option<Timestamp>,
    ) -> Self {
        Self {
            book_id,
            subtitle,
            authors,
            publisher,
            published_date,
            language,
            description,
            identifier,
            created_at,
            updated_at,
            deleted_at,
        }
    }

    pub fn book_id(&self) -> &BookId {
        &self.book_id
    }

    pub fn subtitle(&self) -> Option<&BookSubtitle> {
        self.subtitle.as_ref()
    }

    pub fn authors(&self) -> &[BookAuthor] {
        &self.authors
    }

    pub fn publisher(&self) -> Option<&BookPublisher> {
        self.publisher.as_ref()
    }

    pub fn published_date(&self) -> Option<BookPublishedDate> {
        self.published_date
    }

    pub fn language(&self) -> Option<&BookLanguage> {
        self.language.as_ref()
    }

    pub fn description(&self) -> Option<&BookDescription> {
        self.description.as_ref()
    }

    pub fn identifier(&self) -> Option<&BookIdentifier> {
        self.identifier.as_ref()
    }

    pub fn created_at(&self) -> Timestamp {
        self.created_at
    }

    pub fn updated_at(&self) -> Timestamp {
        self.updated_at
    }

    pub fn deleted_at(&self) -> Option<Timestamp> {
        self.deleted_at
    }

    pub fn status(&self) -> BookStatus {
        match self.deleted_at {
            Some(deleted_at) => BookStatus::Deleted { deleted_at },
            None => BookStatus::Active,
        }
    }

    pub fn mark_deleted(&mut self) -> Result<BookMetadataChange<'_>, BookDomainError> {
        if self.status() != BookStatus::Active {
            return Err(BookDomainError::AlreadyDeleted);
        }
        let now = Timestamp::now();
        self.deleted_at = Some(now);
        self.updated_at = now;
        Ok(BookMetadataChange::new(
            self,
            BookMetadataFields::DELETED_AT | BookMetadataFields::UPDATED_AT,
        ))
    }

    pub fn restore(&mut self) -> Result<BookMetadataChange<'_>, BookDomainError> {
        if self.status() == BookStatus::Active {
            return Err(BookDomainError::NotDeleted);
        }
        self.deleted_at = None;
        self.updated_at = Timestamp::now();
        Ok(BookMetadataChange::new(
            self,
            BookMetadataFields::DELETED_AT | BookMetadataFields::UPDATED_AT,
        ))
    }

    fn update_optional_field<T: PartialEq>(
        &mut self,
        slot: impl FnOnce(&mut Self) -> &mut Option<T>,
        value: Option<T>,
        field: BookMetadataFields,
        unchanged: BookDomainError,
    ) -> Result<BookMetadataChange<'_>, BookDomainError> {
        if self.status() != BookStatus::Active {
            return Err(BookDomainError::AlreadyDeleted);
        }
        let target = slot(self);
        if *target == value {
            return Err(unchanged);
        }
        *target = value;
        self.updated_at = Timestamp::now();
        Ok(BookMetadataChange::new(
            self,
            field | BookMetadataFields::UPDATED_AT,
        ))
    }

    pub fn change_subtitle(
        &mut self,
        subtitle: BookSubtitle,
    ) -> Result<BookMetadataChange<'_>, BookDomainError> {
        self.update_optional_field(
            |s| &mut s.subtitle,
            Some(subtitle),
            BookMetadataFields::SUBTITLE,
            BookDomainError::SubtitleUnchanged,
        )
    }

    pub fn clear_subtitle(&mut self) -> Result<BookMetadataChange<'_>, BookDomainError> {
        self.update_optional_field(
            |s| &mut s.subtitle,
            None,
            BookMetadataFields::SUBTITLE,
            BookDomainError::SubtitleUnchanged,
        )
    }

    pub fn change_authors(
        &mut self,
        authors: Vec<BookAuthor>,
    ) -> Result<BookMetadataChange<'_>, BookDomainError> {
        if self.status() != BookStatus::Active {
            return Err(BookDomainError::AlreadyDeleted);
        }
        BookAuthor::validate_list(&authors)?;
        if self.authors == authors {
            return Err(BookDomainError::AuthorsUnchanged);
        }
        self.authors = authors;
        self.updated_at = Timestamp::now();
        Ok(BookMetadataChange::new(
            self,
            BookMetadataFields::AUTHORS | BookMetadataFields::UPDATED_AT,
        ))
    }

    pub fn clear_authors(&mut self) -> Result<BookMetadataChange<'_>, BookDomainError> {
        if self.status() != BookStatus::Active {
            return Err(BookDomainError::AlreadyDeleted);
        }
        if self.authors.is_empty() {
            return Err(BookDomainError::AuthorsUnchanged);
        }
        self.authors.clear();
        self.updated_at = Timestamp::now();
        Ok(BookMetadataChange::new(
            self,
            BookMetadataFields::AUTHORS | BookMetadataFields::UPDATED_AT,
        ))
    }

    pub fn change_publisher(
        &mut self,
        publisher: BookPublisher,
    ) -> Result<BookMetadataChange<'_>, BookDomainError> {
        self.update_optional_field(
            |s| &mut s.publisher,
            Some(publisher),
            BookMetadataFields::PUBLISHER,
            BookDomainError::PublisherUnchanged,
        )
    }

    pub fn clear_publisher(&mut self) -> Result<BookMetadataChange<'_>, BookDomainError> {
        self.update_optional_field(
            |s| &mut s.publisher,
            None,
            BookMetadataFields::PUBLISHER,
            BookDomainError::PublisherUnchanged,
        )
    }

    pub fn change_published_date(
        &mut self,
        published_date: BookPublishedDate,
    ) -> Result<BookMetadataChange<'_>, BookDomainError> {
        self.update_optional_field(
            |s| &mut s.published_date,
            Some(published_date),
            BookMetadataFields::PUBLISHED_DATE,
            BookDomainError::PublishedDateUnchanged,
        )
    }

    pub fn clear_published_date(&mut self) -> Result<BookMetadataChange<'_>, BookDomainError> {
        self.update_optional_field(
            |s| &mut s.published_date,
            None,
            BookMetadataFields::PUBLISHED_DATE,
            BookDomainError::PublishedDateUnchanged,
        )
    }

    pub fn change_language(
        &mut self,
        language: BookLanguage,
    ) -> Result<BookMetadataChange<'_>, BookDomainError> {
        self.update_optional_field(
            |s| &mut s.language,
            Some(language),
            BookMetadataFields::LANGUAGE,
            BookDomainError::LanguageUnchanged,
        )
    }

    pub fn clear_language(&mut self) -> Result<BookMetadataChange<'_>, BookDomainError> {
        self.update_optional_field(
            |s| &mut s.language,
            None,
            BookMetadataFields::LANGUAGE,
            BookDomainError::LanguageUnchanged,
        )
    }

    pub fn change_description(
        &mut self,
        description: BookDescription,
    ) -> Result<BookMetadataChange<'_>, BookDomainError> {
        self.update_optional_field(
            |s| &mut s.description,
            Some(description),
            BookMetadataFields::DESCRIPTION,
            BookDomainError::DescriptionUnchanged,
        )
    }

    pub fn clear_description(&mut self) -> Result<BookMetadataChange<'_>, BookDomainError> {
        self.update_optional_field(
            |s| &mut s.description,
            None,
            BookMetadataFields::DESCRIPTION,
            BookDomainError::DescriptionUnchanged,
        )
    }

    pub fn change_identifier(
        &mut self,
        identifier: BookIdentifier,
    ) -> Result<BookMetadataChange<'_>, BookDomainError> {
        self.update_optional_field(
            |s| &mut s.identifier,
            Some(identifier),
            BookMetadataFields::IDENTIFIER,
            BookDomainError::IdentifierUnchanged,
        )
    }

    pub fn clear_identifier(&mut self) -> Result<BookMetadataChange<'_>, BookDomainError> {
        self.update_optional_field(
            |s| &mut s.identifier,
            None,
            BookMetadataFields::IDENTIFIER,
            BookDomainError::IdentifierUnchanged,
        )
    }
}

