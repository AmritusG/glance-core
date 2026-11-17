//! Text recognition on a still picture (v0.6.0, MaCrate's measurement scanner): the picture is
//! JPEG-encoded with the OS's encoder and the OS's recogniser reads the JPEG (macOS: ImageIO, then
//! Vision's `VNRecognizeTextRequest`). Every option of the request is the caller's, so a reading
//! can be pinned to the one another program made.

/// The request. [`TextOptions::default`] is Swift MaCrate's pinned text check: JPEG quality 0.9,
/// revision 3, accurate, language correction on, `en-US` only, no minimum text height, no automatic
/// language detection.
#[derive(Clone, Debug, PartialEq)]
pub struct TextOptions {
    /// `kCGImageDestinationLossyCompressionQuality` of the JPEG the recogniser reads (0 to 1).
    pub jpeg_quality: f64,
    /// `VNRecognizeTextRequest.revision`.
    pub revision: usize,
    /// `recognitionLevel`: `.accurate` (true) or `.fast`.
    pub accurate: bool,
    /// `usesLanguageCorrection`.
    pub language_correction: bool,
    /// `recognitionLanguages` (BCP 47 tags, in priority order).
    pub languages: Vec<String>,
    /// `minimumTextHeight`, a fraction of the image height (0 = the recogniser's own floor).
    pub minimum_text_height: f32,
    /// `automaticallyDetectsLanguage`.
    pub automatically_detects_language: bool,
}

impl Default for TextOptions {
    fn default() -> Self {
        TextOptions {
            jpeg_quality: 0.9,
            revision: 3,
            accurate: true,
            language_correction: true,
            languages: vec!["en-US".into()],
            minimum_text_height: 0.0,
            automatically_detects_language: false,
        }
    }
}

/// One recognised line or word group: the recogniser's top candidate, unrounded, in its order.
#[derive(Clone, Debug, PartialEq)]
pub struct TextObservation {
    /// The top candidate's string (`VNRecognizedText.string`).
    pub text: String,
    /// The top candidate's confidence, 0 to 1 (`VNRecognizedText.confidence`).
    pub confidence: f32,
    /// The observation's bounding box (`boundingBox`), normalised to the picture, origin at the
    /// bottom-left corner (Vision's coordinates): x, y, width, height.
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Why a picture could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextError {
    /// No recogniser on this system.
    Unsupported,
    /// The picture could not be made or encoded: `"CGImage"` (bad size or buffer),
    /// `"JPEG encoder unavailable"`, `"JPEG encode failed"`.
    Image(&'static str),
    /// The recogniser failed: its error's description (`NSError.description`).
    Recogniser(String),
}

impl std::fmt::Display for TextError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TextError::Unsupported => f.write_str("text recognition is not supported on this system"),
            TextError::Image(what) => f.write_str(what),
            TextError::Recogniser(why) => f.write_str(why),
        }
    }
}

impl std::error::Error for TextError {}

/// Read the text in a picture: `rgbx` is `width × height` opaque 8-bit sRGB pixels, R, G, B and one
/// ignored byte each, rows tight (`width * 4` bytes, top row first). The picture is JPEG-encoded at
/// `options.jpeg_quality` and the recogniser reads the JPEG with exactly `options`; every observation
/// with a candidate comes back, top candidate only, in the recogniser's order.
///
/// macOS: `CGImage` (8 bits per component, 32 per pixel, `noneSkipLast`, sRGB, no decode array, no
/// interpolation, default intent) → `CGImageDestination` (`public.jpeg`, one image) →
/// `VNImageRequestHandler(data:options: [:])` → `perform([VNRecognizeTextRequest])`. Elsewhere
/// [`TextError::Unsupported`].
pub fn recognise_text(rgbx: &[u8], width: usize, height: usize, options: &TextOptions) -> Result<Vec<TextObservation>, TextError> {
    #[cfg(target_os = "macos")]
    {
        if width == 0 || height == 0 || width.checked_mul(height).and_then(|n| n.checked_mul(4)) != Some(rgbx.len()) {
            return Err(TextError::Image("CGImage"));
        }
        crate::macos_text::recognise(rgbx, width, height, options)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (rgbx, width, height, options);
        Err(TextError::Unsupported)
    }
}
