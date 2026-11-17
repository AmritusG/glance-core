//! `recognise_text` on macOS: CoreGraphics makes the picture, ImageIO encodes the JPEG, Vision reads it —
//! the calls and options Swift MaCrate 0.16.4's text check makes (its `TextChecker` and `Stills.cgImage`).

use crate::text::{TextError, TextObservation, TextOptions};
use objc2::AnyThread;
use objc2::msg_send;
use objc2::rc::{Retained, autoreleasepool};
use objc2_core_foundation::{CFData, CFDictionary, CFMutableData, CFNumber, CFRetained, CFString, CFType};
use objc2_core_graphics::{CGBitmapInfo, CGColorRenderingIntent, CGColorSpace, CGDataProvider, CGImage, CGImageAlphaInfo, kCGColorSpaceSRGB};
use objc2_foundation::{NSArray, NSData, NSDictionary, NSString};
use objc2_image_io::{CGImageDestination, kCGImageDestinationLossyCompressionQuality};
use objc2_vision::{VNImageRequestHandler, VNRecognizeTextRequest, VNRequest, VNRequestTextRecognitionLevel};

/// `width × height` RGBX (checked by the caller) → an opaque sRGB `CGImage` over a copy of the bytes.
fn cg_image(rgbx: &[u8], width: usize, height: usize) -> Option<CFRetained<CGImage>> {
    let data = CFData::from_bytes(rgbx);
    let provider = CGDataProvider::with_cf_data(Some(&data))?;
    // SAFETY: `kCGColorSpaceSRGB` is an immutable CFString constant the framework exports.
    let space = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB }))?;
    // SAFETY: the geometry matches the buffer (`width * 4` bytes per row, `height` rows, checked by the
    // caller); the provider and colour space are valid for the call; `decode` is null (no decode array).
    unsafe {
        CGImage::new(
            width,
            height,
            8,
            32,
            width * 4,
            Some(&space),
            CGBitmapInfo(CGImageAlphaInfo::NoneSkipLast.0),
            Some(&provider),
            std::ptr::null(),
            false,
            CGColorRenderingIntent::RenderingIntentDefault,
        )
    }
}

/// The picture as a JPEG at `quality` (`CGImageDestinationCreateWithData`, `public.jpeg`, one image,
/// `kCGImageDestinationLossyCompressionQuality`).
fn jpeg(image: &CGImage, quality: f64) -> Result<Vec<u8>, TextError> {
    let out = CFMutableData::new(None, 0).ok_or(TextError::Image("JPEG encoder unavailable"))?;
    let kind = CFString::from_static_str("public.jpeg");
    // SAFETY: `out` and `kind` are valid CF objects; no options dictionary.
    let dest = unsafe { CGImageDestination::with_data(&out, &kind, 1, None) }.ok_or(TextError::Image("JPEG encoder unavailable"))?;
    let q = CFNumber::new_f64(quality);
    // SAFETY: `kCGImageDestinationLossyCompressionQuality` is an immutable CFString constant ImageIO exports.
    let key: &CFString = unsafe { kCGImageDestinationLossyCompressionQuality };
    let value: &CFType = &q;
    let props = CFDictionary::<CFString, CFType>::from_slices(&[key], &[value]);
    // SAFETY: `dest` is a live destination with room for one image; the properties dictionary holds a
    // CFString key and a CFNumber value, the types ImageIO documents for this key.
    unsafe { dest.add_image(image, Some(props.as_opaque())) };
    // SAFETY: `dest` is a live destination that has been given its one image.
    if !unsafe { dest.finalize() } {
        return Err(TextError::Image("JPEG encode failed"));
    }
    Ok(out.to_vec())
}

pub(crate) fn recognise(rgbx: &[u8], width: usize, height: usize, options: &TextOptions) -> Result<Vec<TextObservation>, TextError> {
    autoreleasepool(|_| {
        let image = cg_image(rgbx, width, height).ok_or(TextError::Image("CGImage"))?;
        let jpeg = jpeg(&image, options.jpeg_quality)?;

        let request = VNRecognizeTextRequest::new();
        // SAFETY: setting a revision is always allowed; an unsupported one makes `perform` fail with
        // Vision's error, which is returned.
        unsafe { request.setRevision(options.revision) };
        request.setRecognitionLevel(if options.accurate { VNRequestTextRecognitionLevel::Accurate } else { VNRequestTextRecognitionLevel::Fast });
        request.setUsesLanguageCorrection(options.language_correction);
        let languages: Vec<Retained<NSString>> = options.languages.iter().map(|l| NSString::from_str(l)).collect();
        request.setRecognitionLanguages(&NSArray::from_retained_slice(&languages));
        request.setMinimumTextHeight(options.minimum_text_height);
        request.setAutomaticallyDetectsLanguage(options.automatically_detects_language);

        let data = NSData::with_bytes(&jpeg);
        let handler = VNImageRequestHandler::initWithData_options(VNImageRequestHandler::alloc(), &data, &NSDictionary::new());
        let as_request: &VNRequest = &request;
        handler.performRequests_error(&NSArray::from_slice(&[as_request])).map_err(|e| {
            // SAFETY: `-[NSError description]` takes no arguments and returns an NSString (Swift's `"\(error)"`).
            let text: Retained<NSString> = unsafe { msg_send![&*e, description] };
            TextError::Recogniser(text.to_string())
        })?;

        let mut out = Vec::new();
        for observation in request.results().map(|r| r.to_vec()).unwrap_or_default() {
            let Some(top) = observation.topCandidates(1).firstObject() else { continue };
            // SAFETY: `boundingBox` is a plain property read on a live observation.
            let bb = unsafe { observation.boundingBox() };
            out.push(TextObservation {
                text: top.string().to_string(),
                confidence: top.confidence(),
                x: bb.origin.x,
                y: bb.origin.y,
                w: bb.size.width,
                h: bb.size.height,
            });
        }
        Ok(out)
    })
}
