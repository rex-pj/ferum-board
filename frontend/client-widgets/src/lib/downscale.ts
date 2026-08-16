// Shrinks an oversized attachment before it goes on the wire.
//
// **This is a bandwidth optimisation, not a policy.** The server re-encodes
// every upload anyway and owns the real limits; nothing here is trusted or
// relied upon. Every failure path therefore returns the original file rather
// than an error — a browser without `createImageBitmap`, a decode failure, a
// canvas the OS refuses to allocate, all just mean "upload what the user
// picked", which is exactly what happened before this existed.

/// Long-edge cap applied on the client.
///
/// **Deliberately above `DEFAULT_IMAGE_MAX_LONG_EDGE` (2048) in
/// `constants.rs`.** The server's cap is admin-configurable and is the one that
/// decides what gets stored; if this matched it, raising the server setting
/// would silently do nothing for anyone posting through the composer, because
/// the pixels would already be gone. Sitting above it means the client only
/// discards detail no reachable server setting would have kept, and still cuts
/// a 12 MP phone photo by roughly 4×.
const CLIENT_MAX_LONG_EDGE = 3072;

/// Below this, re-encoding costs more than it saves and risks making the file
/// *larger* — a small PNG screenshot turns into a bigger JPEG.
const MIN_BYTES_TO_BOTHER = 1_500_000;

/// JPEG quality for the client's copy.
///
/// High on purpose: the server encodes again at its own (lower) quality, and
/// artefacts introduced here would be baked in before it ever sees the image.
/// The point of this pass is pixel count, not compression.
const CLIENT_QUALITY = 0.92;

export async function shrinkForUpload(file: File): Promise<File> {
  // An animated GIF has no still equivalent — the same reason the server's
  // pipeline passes them straight through. Re-encoding one here would upload a
  // single frame and report success.
  if (file.type === "image/gif") return file;
  if (file.size < MIN_BYTES_TO_BOTHER) return file;
  if (typeof createImageBitmap !== "function") return file;

  let bitmap: ImageBitmap;
  try {
    bitmap = await createImageBitmap(file);
  } catch {
    return file;
  }

  try {
    const longEdge = Math.max(bitmap.width, bitmap.height);
    if (longEdge <= CLIENT_MAX_LONG_EDGE) return file;

    const ratio = CLIENT_MAX_LONG_EDGE / longEdge;
    const width = Math.max(1, Math.round(bitmap.width * ratio));
    const height = Math.max(1, Math.round(bitmap.height * ratio));

    const canvas = document.createElement("canvas");
    canvas.width = width;
    canvas.height = height;
    const context = canvas.getContext("2d");
    if (!context) return file;
    context.drawImage(bitmap, 0, 0, width, height);

    const blob = await new Promise<Blob | null>((resolve) =>
      canvas.toBlob(resolve, "image/jpeg", CLIENT_QUALITY),
    );
    // `toBlob` yields null when the canvas is tainted or too large for the
    // platform. Both mean "use the original".
    if (!blob || blob.size >= file.size) return file;

    return new File([blob], renameToJpeg(file.name), {
      type: "image/jpeg",
      lastModified: file.lastModified,
    });
  } finally {
    // Frees the decoded bitmap immediately instead of waiting for GC, which
    // matters when the source was 12 MP.
    bitmap.close();
  }
}

/// The bytes are JPEG now, so the name must say so — the server derives nothing
/// from it, but the user sees it in the composer and in error messages.
function renameToJpeg(name: string): string {
  return name.replace(/\.[^./\\]+$/, "") + ".jpg";
}
