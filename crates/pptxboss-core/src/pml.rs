//! PresentationML part identities (ECMA-376 Part 1, clause 13; Part 4,
//! clause 11): content types and relationship types, with both the
//! Transitional and Strict relationship URIs recognized.

/// Content types of the parts a presentation package contains.
pub mod content_type {
    pub const PRESENTATION: &str =
        "application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml";
    pub const SLIDESHOW: &str =
        "application/vnd.openxmlformats-officedocument.presentationml.slideshow.main+xml";
    pub const TEMPLATE: &str =
        "application/vnd.openxmlformats-officedocument.presentationml.template.main+xml";
    pub const PRESENTATION_MACRO: &str =
        "application/vnd.ms-powerpoint.presentation.macroEnabled.main+xml";
    pub const SLIDESHOW_MACRO: &str =
        "application/vnd.ms-powerpoint.slideshow.macroEnabled.main+xml";
    pub const TEMPLATE_MACRO: &str = "application/vnd.ms-powerpoint.template.macroEnabled.main+xml";
    pub const ADDIN_MACRO: &str = "application/vnd.ms-powerpoint.addin.macroEnabled.main+xml";
    pub const SLIDE: &str =
        "application/vnd.openxmlformats-officedocument.presentationml.slide+xml";
    pub const SLIDE_LAYOUT: &str =
        "application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml";
    pub const SLIDE_MASTER: &str =
        "application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml";
    pub const NOTES_SLIDE: &str =
        "application/vnd.openxmlformats-officedocument.presentationml.notesSlide+xml";
    pub const NOTES_MASTER: &str =
        "application/vnd.openxmlformats-officedocument.presentationml.notesMaster+xml";
    pub const HANDOUT_MASTER: &str =
        "application/vnd.openxmlformats-officedocument.presentationml.handoutMaster+xml";
    pub const PRES_PROPS: &str =
        "application/vnd.openxmlformats-officedocument.presentationml.presProps+xml";
    pub const VIEW_PROPS: &str =
        "application/vnd.openxmlformats-officedocument.presentationml.viewProps+xml";
    pub const TABLE_STYLES: &str =
        "application/vnd.openxmlformats-officedocument.presentationml.tableStyles+xml";
    pub const COMMENT_AUTHORS: &str =
        "application/vnd.openxmlformats-officedocument.presentationml.commentAuthors+xml";
    pub const COMMENTS: &str =
        "application/vnd.openxmlformats-officedocument.presentationml.comments+xml";
    pub const MODERN_COMMENTS: &str = "application/vnd.ms-powerpoint.comments+xml";
    pub const AUTHORS: &str = "application/vnd.ms-powerpoint.authors+xml";
    pub const TAGS: &str = "application/vnd.openxmlformats-officedocument.presentationml.tags+xml";
    pub const THEME: &str = "application/vnd.openxmlformats-officedocument.theme+xml";
    pub const THEME_OVERRIDE: &str =
        "application/vnd.openxmlformats-officedocument.themeOverride+xml";
    pub const CHART: &str = "application/vnd.openxmlformats-officedocument.drawingml.chart+xml";
    pub const DIAGRAM_DATA: &str =
        "application/vnd.openxmlformats-officedocument.drawingml.diagramData+xml";
    pub const EXTENDED_PROPERTIES: &str =
        "application/vnd.openxmlformats-officedocument.extended-properties+xml";
    pub const CUSTOM_PROPERTIES: &str =
        "application/vnd.openxmlformats-officedocument.custom-properties+xml";

    /// Whether `content_type` is one of the Presentation part's main types (13.3.6 plus the macro-enabled forms).
    pub fn is_presentation_main(content_type: &str) -> bool {
        [
            PRESENTATION,
            SLIDESHOW,
            TEMPLATE,
            PRESENTATION_MACRO,
            SLIDESHOW_MACRO,
            TEMPLATE_MACRO,
            ADDIN_MACRO,
        ]
        .iter()
        .any(|known| known.eq_ignore_ascii_case(content_type))
    }
}

const TRANSITIONAL_PREFIX: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/";
const STRICT_PREFIX: &str = "http://purl.oclc.org/ooxml/officeDocument/relationships/";
const PACKAGE_PREFIX: &str = "http://schemas.openxmlformats.org/package/2006/relationships/";
const MS_MEDIA: &str = "http://schemas.microsoft.com/office/2007/relationships/media";
const MS_COMMENTS: &str = "http://schemas.microsoft.com/office/2018/10/relationships/comments";
const MS_AUTHORS: &str = "http://schemas.microsoft.com/office/2018/10/relationships/authors";

/// The relationship types a reader dispatches on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum RelKind {
    OfficeDocument,
    Slide,
    SlideLayout,
    SlideMaster,
    NotesSlide,
    NotesMaster,
    HandoutMaster,
    PresProps,
    ViewProps,
    TableStyles,
    CommentAuthors,
    Comments,
    /// The 2018 threaded comments part (`p188:cmLst`).
    ModernComments,
    /// The authors part of the 2018 comments.
    Authors,
    Tags,
    Theme,
    ThemeOverride,
    Image,
    Hyperlink,
    Chart,
    ChartUserShapes,
    DiagramData,
    DiagramLayout,
    DiagramQuickStyle,
    DiagramColors,
    OleObject,
    Package,
    Audio,
    Video,
    Media,
    Font,
    PrinterSettings,
    VmlDrawing,
    Control,
    CustomXml,
    SlideUpdateInfo,
    CoreProperties,
    ExtendedProperties,
    CustomProperties,
    Thumbnail,
    Other,
}

impl RelKind {
    /// Classifies a relationship type URI (Transitional or Strict).
    pub fn of(rel_type: &str) -> RelKind {
        if let Some(tail) = rel_type
            .strip_prefix(TRANSITIONAL_PREFIX)
            .or_else(|| rel_type.strip_prefix(STRICT_PREFIX))
        {
            return match tail {
                "officeDocument" => RelKind::OfficeDocument,
                "slide" => RelKind::Slide,
                "slideLayout" => RelKind::SlideLayout,
                "slideMaster" => RelKind::SlideMaster,
                "notesSlide" => RelKind::NotesSlide,
                "notesMaster" => RelKind::NotesMaster,
                "handoutMaster" => RelKind::HandoutMaster,
                "presProps" => RelKind::PresProps,
                "viewProps" => RelKind::ViewProps,
                "tableStyles" => RelKind::TableStyles,
                "commentAuthors" => RelKind::CommentAuthors,
                "comments" => RelKind::Comments,
                "tags" => RelKind::Tags,
                "theme" => RelKind::Theme,
                "themeOverride" => RelKind::ThemeOverride,
                "image" => RelKind::Image,
                "hyperlink" => RelKind::Hyperlink,
                "chart" => RelKind::Chart,
                "chartUserShapes" => RelKind::ChartUserShapes,
                "diagramData" => RelKind::DiagramData,
                "diagramLayout" => RelKind::DiagramLayout,
                "diagramQuickStyle" => RelKind::DiagramQuickStyle,
                "diagramColors" => RelKind::DiagramColors,
                "oleObject" => RelKind::OleObject,
                "package" => RelKind::Package,
                "audio" => RelKind::Audio,
                "video" => RelKind::Video,
                "font" => RelKind::Font,
                "printerSettings" => RelKind::PrinterSettings,
                "vmlDrawing" => RelKind::VmlDrawing,
                "control" => RelKind::Control,
                "customXml" => RelKind::CustomXml,
                "slideUpdateInfo" => RelKind::SlideUpdateInfo,
                "extended-properties" | "extendedProperties" => RelKind::ExtendedProperties,
                "custom-properties" | "customProperties" => RelKind::CustomProperties,
                _ => RelKind::Other,
            };
        }
        if let Some(tail) = rel_type.strip_prefix(PACKAGE_PREFIX) {
            return match tail {
                "metadata/core-properties" => RelKind::CoreProperties,
                "metadata/thumbnail" => RelKind::Thumbnail,
                _ => RelKind::Other,
            };
        }
        match rel_type {
            MS_MEDIA => RelKind::Media,
            MS_COMMENTS => RelKind::ModernComments,
            MS_AUTHORS => RelKind::Authors,
            _ => RelKind::Other,
        }
    }

    /// The Transitional URI a writer emits for this kind.
    pub fn uri(self) -> &'static str {
        match self {
            RelKind::OfficeDocument => "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument",
            RelKind::Slide => "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide",
            RelKind::SlideLayout => "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout",
            RelKind::SlideMaster => "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster",
            RelKind::NotesSlide => "http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesSlide",
            RelKind::NotesMaster => "http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesMaster",
            RelKind::HandoutMaster => "http://schemas.openxmlformats.org/officeDocument/2006/relationships/handoutMaster",
            RelKind::PresProps => "http://schemas.openxmlformats.org/officeDocument/2006/relationships/presProps",
            RelKind::ViewProps => "http://schemas.openxmlformats.org/officeDocument/2006/relationships/viewProps",
            RelKind::TableStyles => "http://schemas.openxmlformats.org/officeDocument/2006/relationships/tableStyles",
            RelKind::CommentAuthors => "http://schemas.openxmlformats.org/officeDocument/2006/relationships/commentAuthors",
            RelKind::Comments => "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments",
            RelKind::Tags => "http://schemas.openxmlformats.org/officeDocument/2006/relationships/tags",
            RelKind::Theme => "http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme",
            RelKind::ThemeOverride => "http://schemas.openxmlformats.org/officeDocument/2006/relationships/themeOverride",
            RelKind::Image => "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image",
            RelKind::Hyperlink => "http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink",
            RelKind::Chart => "http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart",
            RelKind::ChartUserShapes => "http://schemas.openxmlformats.org/officeDocument/2006/relationships/chartUserShapes",
            RelKind::DiagramData => "http://schemas.openxmlformats.org/officeDocument/2006/relationships/diagramData",
            RelKind::DiagramLayout => "http://schemas.openxmlformats.org/officeDocument/2006/relationships/diagramLayout",
            RelKind::DiagramQuickStyle => "http://schemas.openxmlformats.org/officeDocument/2006/relationships/diagramQuickStyle",
            RelKind::DiagramColors => "http://schemas.openxmlformats.org/officeDocument/2006/relationships/diagramColors",
            RelKind::OleObject => "http://schemas.openxmlformats.org/officeDocument/2006/relationships/oleObject",
            RelKind::Package => "http://schemas.openxmlformats.org/officeDocument/2006/relationships/package",
            RelKind::Audio => "http://schemas.openxmlformats.org/officeDocument/2006/relationships/audio",
            RelKind::Video => "http://schemas.openxmlformats.org/officeDocument/2006/relationships/video",
            RelKind::Media => MS_MEDIA,
            RelKind::ModernComments => MS_COMMENTS,
            RelKind::Authors => MS_AUTHORS,
            RelKind::Font => "http://schemas.openxmlformats.org/officeDocument/2006/relationships/font",
            RelKind::PrinterSettings => "http://schemas.openxmlformats.org/officeDocument/2006/relationships/printerSettings",
            RelKind::VmlDrawing => "http://schemas.openxmlformats.org/officeDocument/2006/relationships/vmlDrawing",
            RelKind::Control => "http://schemas.openxmlformats.org/officeDocument/2006/relationships/control",
            RelKind::CustomXml => "http://schemas.openxmlformats.org/officeDocument/2006/relationships/customXml",
            RelKind::SlideUpdateInfo => "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideUpdateInfo",
            RelKind::CoreProperties => "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties",
            RelKind::ExtendedProperties => "http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties",
            RelKind::CustomProperties => "http://schemas.openxmlformats.org/officeDocument/2006/relationships/custom-properties",
            RelKind::Thumbnail => "http://schemas.openxmlformats.org/package/2006/relationships/metadata/thumbnail",
            RelKind::Other => "",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transitional_and_strict_relationship_types_classify_alike() {
        assert_eq!(
            RelKind::of(
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide"
            ),
            RelKind::Slide
        );
        assert_eq!(
            RelKind::of("http://purl.oclc.org/ooxml/officeDocument/relationships/slide"),
            RelKind::Slide
        );
        assert_eq!(RelKind::of("http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties"), RelKind::CoreProperties);
        assert_eq!(RelKind::of("http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties"), RelKind::ExtendedProperties);
        assert_eq!(
            RelKind::of(
                "http://purl.oclc.org/ooxml/officeDocument/relationships/extendedProperties"
            ),
            RelKind::ExtendedProperties
        );
        assert_eq!(RelKind::of(MS_MEDIA), RelKind::Media);
        assert_eq!(RelKind::of("urn:something"), RelKind::Other);
        assert_eq!(RelKind::of(RelKind::NotesSlide.uri()), RelKind::NotesSlide);
    }

    #[test]
    fn presentation_main_types() {
        assert!(content_type::is_presentation_main(
            content_type::PRESENTATION
        ));
        assert!(content_type::is_presentation_main(
            content_type::SLIDESHOW_MACRO
        ));
        assert!(content_type::is_presentation_main(
            &content_type::TEMPLATE.to_ascii_uppercase()
        ));
        assert!(!content_type::is_presentation_main(content_type::SLIDE));
    }
}
