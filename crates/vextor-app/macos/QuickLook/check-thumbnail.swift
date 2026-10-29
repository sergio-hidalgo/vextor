// Asks Quick Look for the thumbnail of each file given, the way Finder
// does (`QLThumbnailGenerator`), and prints one line per file:
//
//   thumbnail <width>x<height> <file>    the extension drew the artwork
//   icon <file>                          no thumbnail: Finder shows the icon
//
// With `-o <dir>`, each thumbnail is also saved there as <name>.png.
// Used by scripts/check-quicklook.sh (spec 14d): `qlmanage -t` no longer
// drives app extensions on current macOS.

import Foundation
import ImageIO
import QuickLookThumbnailing
import UniformTypeIdentifiers

var args = Array(CommandLine.arguments.dropFirst())
var outDir: URL?
if args.first == "-o", args.count >= 2 {
    outDir = URL(fileURLWithPath: args[1], isDirectory: true)
    args.removeFirst(2)
}

for path in args {
    let url = URL(fileURLWithPath: path)
    let request = QLThumbnailGenerator.Request(
        fileAt: url, size: CGSize(width: 512, height: 512), scale: 1,
        representationTypes: .thumbnail)
    let done = DispatchSemaphore(value: 0)
    var line = "icon \(path)"
    QLThumbnailGenerator.shared.generateBestRepresentation(for: request) { rep, _ in
        if let image = rep?.cgImage {
            line = "thumbnail \(image.width)x\(image.height) \(path)"
            if let outDir {
                let png = outDir.appendingPathComponent(url.deletingPathExtension().lastPathComponent + ".png")
                if let dest = CGImageDestinationCreateWithURL(png as CFURL, UTType.png.identifier as CFString, 1, nil) {
                    CGImageDestinationAddImage(dest, image, nil)
                    CGImageDestinationFinalize(dest)
                }
            }
        }
        done.signal()
    }
    if done.wait(timeout: .now() + 30) == .timedOut {
        line = "timeout \(path)"
    }
    print(line)
}
