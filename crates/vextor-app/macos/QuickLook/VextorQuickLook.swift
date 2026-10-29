// Finder previews of .vxtr documents (spec 14d).
//
// Vextor stores a small PNG of the artwork in every document it saves
// (`preview_png`). These Quick Look extensions only read that picture back:
// they never render a document, write nothing and make no network request.
// Every file is untrusted: anything unexpected (too big, not gzip or JSON,
// no preview, not a PNG) fails the request and Finder shows the document
// icon instead.
//
// scripts/bundle-macos.sh compiles this one file twice: VextorThumbnail.appex
// (principal class ThumbnailProvider: Finder icons, list and column views)
// and VextorPreview.appex (PreviewViewController: the column view pane and
// the space bar). A preview extension that hands Quick Look an image
// (`QLPreviewReply`) needs macOS 12; this view-based one runs on macOS 11.

import Cocoa
import Compression
import ImageIO
import Quartz
import QuickLookThumbnailing

/// Reads the preview picture out of a .vxtr file.
enum VextorFile {
    /// Largest file read, and largest unpacked JSON: Vextor's own limit.
    static let maxBytes = 50 * 1024 * 1024
    /// Largest preview PNG accepted. Vextor writes 512 px pictures of a
    /// few dozen KB.
    static let maxPNGBytes = 4 * 1024 * 1024
    /// Largest preview side accepted, in pixels.
    static let maxSide = 2048

    struct Failure: Error {}

    /// The preview of the document at `url`; `nil` when there is none or
    /// the file is not what it should be.
    static func preview(at url: URL) -> CGImage? {
        guard let file = read(url),
              let json = isGzip(file) ? gunzip(file) : file,
              let png = previewPNG(json) else { return nil }
        return decodePNG(png)
    }

    /// The file's bytes, refusing one over `maxBytes`.
    private static func read(_ url: URL) -> Data? {
        guard let size = try? url.resourceValues(forKeys: [.fileSizeKey]).fileSize,
              size <= maxBytes,
              let handle = try? FileHandle(forReadingFrom: url) else { return nil }
        defer { try? handle.close() }
        guard let data = try? handle.read(upToCount: maxBytes + 1),
              data.count <= maxBytes else { return nil }
        return data
    }

    private static func isGzip(_ data: Data) -> Bool {
        data.count >= 2 && data[data.startIndex] == 0x1F && data[data.startIndex + 1] == 0x8B
    }

    /// Unpacks a gzip file (RFC 1952), stopping at `maxBytes` of output.
    private static func gunzip(_ data: Data) -> Data? {
        let bytes = [UInt8](data)
        // ID1 ID2 CM FLG MTIME(4) XFL OS, then the optional fields FLG names.
        guard bytes.count >= 18, bytes[2] == 8 else { return nil }
        let flags = bytes[3]
        guard flags & 0xE0 == 0 else { return nil }
        var start = 10
        if flags & 0x04 != 0 { // FEXTRA
            guard start + 2 <= bytes.count else { return nil }
            start += 2 + (Int(bytes[start]) | Int(bytes[start + 1]) << 8)
        }
        for flag: UInt8 in [0x08, 0x10] where flags & flag != 0 { // FNAME, FCOMMENT
            guard let end = bytes[min(start, bytes.count)...].firstIndex(of: 0) else { return nil }
            start = end + 1
        }
        if flags & 0x02 != 0 { start += 2 } // FHCRC
        // The deflate data, without the CRC32 and size trailer.
        guard start < bytes.count - 8 else { return nil }
        return inflate(bytes[start..<(bytes.count - 8)])
    }

    /// Raw deflate data unpacked, or `nil` when it is damaged, cut off or
    /// unpacks to more than `maxBytes`.
    private static func inflate(_ input: ArraySlice<UInt8>) -> Data? {
        let stream = UnsafeMutablePointer<compression_stream>.allocate(capacity: 1)
        defer { stream.deallocate() }
        guard compression_stream_init(stream, COMPRESSION_STREAM_DECODE, COMPRESSION_ZLIB)
            == COMPRESSION_STATUS_OK else { return nil }
        defer { compression_stream_destroy(stream) }

        let chunk = 1 << 16
        let buffer = UnsafeMutablePointer<UInt8>.allocate(capacity: chunk)
        defer { buffer.deallocate() }
        var output = Data()
        return input.withUnsafeBufferPointer { src -> Data? in
            guard let base = src.baseAddress else { return nil }
            stream.pointee.src_ptr = base
            stream.pointee.src_size = src.count
            while true {
                let unread = stream.pointee.src_size
                stream.pointee.dst_ptr = buffer
                stream.pointee.dst_size = chunk
                let status = compression_stream_process(
                    stream, Int32(COMPRESSION_STREAM_FINALIZE.rawValue))
                let produced = chunk - stream.pointee.dst_size
                guard output.count + produced <= maxBytes else { return nil }
                output.append(buffer, count: produced)
                switch status {
                case COMPRESSION_STATUS_END: return output
                case COMPRESSION_STATUS_OK:
                    // No progress: the data is cut off.
                    if produced == 0 && stream.pointee.src_size == unread { return nil }
                default: return nil
                }
            }
        }
    }

    /// Only the fields read; everything else in the document is skipped.
    private struct Fields: Decodable {
        let format_version: Int
        let preview_png: String?
    }

    /// The PNG bytes of the document's `preview_png`.
    private static func previewPNG(_ json: Data) -> Data? {
        guard let fields = try? JSONDecoder().decode(Fields.self, from: json),
              let text = fields.preview_png,
              text.utf8.count <= maxPNGBytes * 4 / 3 + 4,
              let png = Data(base64Encoded: text),
              png.count <= maxPNGBytes else { return nil }
        return png
    }

    /// `png` decoded, when it is a PNG no bigger than `maxSide` a side; the
    /// size is read from the header before any pixel is decoded.
    private static func decodePNG(_ png: Data) -> CGImage? {
        let options = [kCGImageSourceTypeIdentifierHint: "public.png"] as CFDictionary
        guard let source = CGImageSourceCreateWithData(png as CFData, options),
              CGImageSourceGetType(source) as String? == "public.png",
              CGImageSourceGetCount(source) == 1,
              let props = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any],
              let width = props[kCGImagePropertyPixelWidth] as? Int,
              let height = props[kCGImagePropertyPixelHeight] as? Int,
              (1...maxSide).contains(width), (1...maxSide).contains(height) else { return nil }
        return CGImageSourceCreateImageAtIndex(source, 0, nil)
    }
}

/// Finder's icons for .vxtr files: the saved picture, fitted.
final class ThumbnailProvider: QLThumbnailProvider {
    override func provideThumbnail(
        for request: QLFileThumbnailRequest,
        _ handler: @escaping (QLThumbnailReply?, Error?) -> Void
    ) {
        guard let image = VextorFile.preview(at: request.fileURL) else {
            handler(nil, VextorFile.Failure())
            return
        }
        let (w, h) = (CGFloat(image.width), CGFloat(image.height))
        let scale = min(request.maximumSize.width / w, request.maximumSize.height / h)
        let size = CGSize(width: max(1, (w * scale).rounded()), height: max(1, (h * scale).rounded()))
        handler(QLThumbnailReply(contextSize: size) { context in
            context.interpolationQuality = .high
            context.draw(image, in: CGRect(origin: .zero, size: size))
            return true
        }, nil)
    }
}

/// The column view pane and the space bar: the saved picture in a view.
final class PreviewViewController: NSViewController, QLPreviewingController {
    private let imageView = NSImageView()

    override func loadView() {
        imageView.imageScaling = .scaleProportionallyUpOrDown
        view = imageView
    }

    func preparePreviewOfFile(at url: URL, completionHandler handler: @escaping (Error?) -> Void) {
        guard let image = VextorFile.preview(at: url) else {
            handler(VextorFile.Failure())
            return
        }
        let size = NSSize(width: image.width, height: image.height)
        imageView.image = NSImage(cgImage: image, size: size)
        preferredContentSize = size
        handler(nil)
    }
}
