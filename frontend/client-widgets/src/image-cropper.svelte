<svelte:options customElement={{ tag: "ferum-image-cropper", shadow: "none" }} />

<!--
  Pick an image, frame it, upload it.

  **The gesture handling is `svelte-easy-crop`, not ours.** Pan, pinch, wheel
  zoom, zoom-to-pointer and the Safari `gesturestart`/`gesturechange` suppression
  are the parts that break quietly on real devices and cannot be covered by the
  tests in this repo, so they are borrowed rather than written. What stays here
  is everything that is *this application's* policy: which files are accepted,
  which endpoint receives them, and what shape the crop rectangle takes on the
  wire.

  **The original file is uploaded, not a cropped canvas.** Only the rectangle
  travels alongside it, and the server does the single crop-and-encode. Cropping
  on a canvas first would mean two lossy encodes for one image, and the server
  re-encodes regardless — so the client's copy would be pure loss. It also keeps
  this widget honest: the rect is a *hint*, clamped server-side by
  `frame::clamp_crop`, and no pixel decision here has to be trusted.

  **The frame is fixed and the image moves behind it.** Every target this serves
  has an aspect the layout dictates, so a free-form rectangle would only let the
  user choose a shape the server then overrides. Where several ratios are
  legitimate, `aspect` takes a list and the user picks one.
-->

<script lang="ts">
  import Cropper from "svelte-easy-crop";
  import { t } from "./lib/i18n";

  let {
    endpoint = "",
    field = "file",
    aspect = "1",
    "max-bytes": maxBytes = "5242880",
    shape = "rect",
    label = "",
    "button-class": buttonClass = "btn btn-outline-secondary btn-sm",
  } = $props<{
    endpoint?: string;
    field?: string;
    aspect?: string;
    "max-bytes"?: string;
    shape?: string;
    label?: string;
    "button-class"?: string;
  }>();

  const ACCEPTED = ["image/jpeg", "image/jpg", "image/png", "image/webp", "image/gif"];

  const ratios = $derived(
    aspect
      .split(",")
      .map((r) => Number.parseFloat(r.trim()))
      .filter((r) => Number.isFinite(r) && r > 0),
  );

  let fileInput: HTMLInputElement | undefined = $state();

  let file: File | null = $state(null);
  let src = $state("");
  let ratioIndex = $state(0);
  let busy = $state(false);
  let error = $state("");

  // Bound to the cropper. `pixels` arrives in source-image coordinates, which
  // is exactly what the API takes — no conversion, and therefore no arithmetic
  // of ours between the user's gesture and the request.
  let crop = $state({ x: 0, y: 0 });
  let zoom = $state(1);
  let minZoom = $state(1);
  let maxZoom = $state(3);
  let pixels: { x: number; y: number; width: number; height: number } | null = $state(null);

  const ratio = $derived(ratios[ratioIndex] ?? 1);
  const open = $derived(src !== "");

  function pick() {
    error = "";
    fileInput?.click();
  }

  function onFile(event: Event) {
    const input = event.target as HTMLInputElement;
    const chosen = input.files?.[0];
    // Cleared here rather than after upload: picking the same file twice in a
    // row fires no `change` event otherwise, and the widget looks broken.
    input.value = "";
    if (!chosen) return;

    if (!ACCEPTED.includes(chosen.type)) {
      error = t("js-image-invalid-type");
      return;
    }
    if (chosen.size > Number.parseInt(maxBytes, 10)) {
      error = t("js-image-too-large");
      return;
    }

    file = chosen;
    crop = { x: 0, y: 0 };
    zoom = 1;
    pixels = null;
    ratioIndex = 0;
    src = URL.createObjectURL(chosen);
  }

  function close() {
    if (src) URL.revokeObjectURL(src);
    src = "";
    file = null;
    pixels = null;
    busy = false;
  }

  /// Arrow keys pan; the zoom slider already covers the other axis.
  ///
  /// `svelte-easy-crop` makes its container focusable but binds no key handler,
  /// so without this a keyboard user can tab to the cropper and nothing
  /// happens — which WCAG 2.1 AA (NF-UX-04) does not allow. Handled on the
  /// wrapper so the event bubbles up from the library's own element and no
  /// second tab stop is introduced.
  ///
  /// **What is and is not clamped**, checked against the library's source
  /// rather than assumed. `restrictPosition` runs in two places: the drag path,
  /// and `emitCropData` — which re-clamps before computing the rectangle it
  /// hands to `oncropcomplete`. It does *not* run as an effect on `crop`.
  ///
  /// So writing `crop` here bypasses the clamp on the **displayed** position
  /// but not on the **emitted** one: the rectangle uploaded is always inside
  /// the image, whatever this does. The residual cost is cosmetic — held long
  /// enough, the arrow keys can walk the preview slightly past the edge while
  /// the crop that gets stored stays put. The step is deliberately small so
  /// that takes sustained effort rather than one tap.
  ///
  /// Clamping properly would need `imageSize` and `cropperSize`, which the
  /// component keeps internal, and `helpers.restrictPosition` is not on the
  /// package's `exports` map. Reaching past either would couple this widget to
  /// the library's private shape for a preview nudge.
  function onKeyDown(event: KeyboardEvent) {
    const pan = { ArrowLeft: [1, 0], ArrowRight: [-1, 0], ArrowUp: [0, 1], ArrowDown: [0, -1] }[
      event.key
    ];
    if (!pan) return;
    event.preventDefault();
    const step = event.shiftKey ? 24 : 6;
    crop = { x: crop.x + pan[0] * step, y: crop.y + pan[1] * step };
  }

  async function save() {
    if (!file || !endpoint || busy) return;
    busy = true;
    error = "";

    const form = new FormData();
    form.append(field, file);
    // Absent only if the image never finished loading, in which case the server
    // centre-crops — the same result every upload got before this widget
    // existed, so there is nothing to fail over.
    if (pixels) {
      form.append("crop_x", String(Math.max(0, Math.round(pixels.x))));
      form.append("crop_y", String(Math.max(0, Math.round(pixels.y))));
      form.append("crop_w", String(Math.max(1, Math.round(pixels.width))));
      form.append("crop_h", String(Math.max(1, Math.round(pixels.height))));
    }

    try {
      const response = await fetch(endpoint, { method: "POST", body: form });
      if (response.ok) {
        // Reload rather than patch the DOM: the uploaded image appears in the
        // nav, the profile card and the page body, and this widget knows about
        // none of them.
        window.location.reload();
        return;
      }
      const body = await response.json().catch(() => ({}));
      error = body?.error?.message || t("js-upload-failed");
    } catch {
      error = t("js-network-error");
    }
    busy = false;
  }
</script>

<button type="button" class={buttonClass} onclick={pick}>
  <i class="fa-solid fa-crop-simple me-1" aria-hidden="true"></i>{label || t("js-choose-image")}
</button>

<input
  bind:this={fileInput}
  type="file"
  class="d-none"
  accept="image/jpeg,image/png,image/webp,image/gif"
  onchange={onFile}
/>

{#if error && !open}
  <div class="small text-danger mt-1">{error}</div>
{/if}

{#if open}
  <!-- Not a Bootstrap modal instance: this needs no JS from the framework, and
       constructing one would tie the widget to a global a theme may not load.
       Bootstrap's *classes* still apply — these widgets render into the light
       DOM (`shadow: "none"`). -->
  <div
    class="ferum-cropper-backdrop"
    role="dialog"
    aria-modal="true"
    aria-label={t("js-crop-image")}
  >
    <div class="ferum-cropper-panel card shadow-lg">
      <div class="card-header d-flex align-items-center justify-content-between">
        <span class="fw-semibold">{t("js-crop-image")}</span>
        <button type="button" class="btn-close" aria-label={t("js-cancel")} onclick={close}
        ></button>
      </div>

      <div class="card-body">
        <!-- The cropper positions itself absolutely and fills this box, so the
             height has to come from here. -->
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="ferum-cropper-stage" onkeydown={onKeyDown}>
          <Cropper
            image={src}
            bind:crop
            bind:zoom
            bind:minZoom
            bind:maxZoom
            aspect={ratio}
            cropShape={shape === "circle" ? "round" : "rect"}
            showGrid={shape !== "circle"}
            tabindex={0}
            oncropcomplete={(event) => (pixels = event.pixels)}
          />
        </div>

        {#if ratios.length > 1}
          <div class="btn-group btn-group-sm mt-3 d-flex flex-wrap" role="group">
            {#each ratios as r, i}
              <button
                type="button"
                class="btn {i === ratioIndex ? 'btn-primary' : 'btn-outline-secondary'}"
                onclick={() => (ratioIndex = i)}
              >
                {r === 1 ? "1:1" : r.toFixed(2).replace(/\.?0+$/, "") + ":1"}
              </button>
            {/each}
          </div>
        {/if}

        <label class="form-label small mt-3 mb-1" for="ferum-cropper-zoom">{t("js-zoom")}</label>
        <input
          id="ferum-cropper-zoom"
          type="range"
          class="form-range"
          min={minZoom}
          max={maxZoom}
          step={(maxZoom - minZoom) / 100}
          bind:value={zoom}
        />
        <div class="form-text">{t("js-cropper-hint")}</div>

        {#if error}
          <div class="alert alert-danger py-2 px-3 small mt-2 mb-0">{error}</div>
        {/if}
      </div>

      <div class="card-footer d-flex justify-content-end gap-2">
        <button type="button" class="btn btn-outline-secondary" onclick={close} disabled={busy}>
          {t("js-cancel")}
        </button>
        <button type="button" class="btn btn-primary" onclick={save} disabled={busy}>
          {#if busy}
            <i class="fa-solid fa-spinner fa-spin me-1" aria-hidden="true"></i>
          {/if}
          {t("js-save")}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .ferum-cropper-backdrop {
    position: fixed;
    inset: 0;
    z-index: 1055; /* Bootstrap's modal layer */
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 1rem;
    background: rgba(0, 0, 0, 0.6);
    overflow-y: auto;
  }
  .ferum-cropper-panel {
    width: 100%;
    max-width: 560px;
  }
  .ferum-cropper-stage {
    position: relative;
    /* Capped against the viewport so the Save button stays reachable on a
       landscape phone, where a tall crop box would push it below the fold. */
    height: min(48vh, 320px);
    background: var(--bs-secondary-bg, #e9ecef);
    border-radius: var(--bs-border-radius, 0.375rem);
    overflow: hidden;
  }
  /* 44px tap targets on mobile — NF-UX-02. */
  @media (max-width: 767.98px) {
    .ferum-cropper-panel :global(.btn) {
      min-height: 44px;
    }
  }
</style>
