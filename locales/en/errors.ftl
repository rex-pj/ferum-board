# Error messages — English (source catalog).
#
# Keys are `error-<code>` where <code> is the machine error code returned in
# `{"error": {"code": ...}}`, kebab-cased. The code is part of the API contract;
# the text below is not. Translators may reorder, split, or reword freely.
#
# This file is the ONLY source of English error text. Nothing in the Rust code
# duplicates it — `AppError::into_response` emits the code and the
# `translate_errors` middleware resolves it here.
#
# Placeables like { $seconds } are supplied by the error's arguments. Do not
# add a placeable the code does not pass, or it will render as its own name.

## ─── Generic HTTP-level ──────────────────────────────────────────────────────

error-unauthorized = You need to sign in to do that.
error-not-found = We couldn't find what you were looking for.
error-internal-error = Something went wrong on our end. Please try again.
error-validation-error = Some of the information you entered isn't valid.
error-rate-limit-exceeded =
    Too many requests. Please try again in { $seconds ->
        [one] 1 second
       *[other] { $seconds } seconds
    }.

## ─── Permissions & trust ─────────────────────────────────────────────────────

error-permission-denied = You don't have permission to do that.
error-trust-level-insufficient =
    Your account needs a higher trust level to do this. Keep participating to level up.
error-not-author = You can only do that with your own content.
error-tag-create-permission-required = You don't have permission to create new tags.
error-cannot-grant-permissions-you-lack =
    You can't grant a permission you don't have yourself.
error-cannot-modify-system-role-permissions = Built-in role permissions can't be modified.
error-cannot-delete-system-role = Built-in roles can't be deleted.
error-cannot-remove-last-admin = You can't remove the last administrator.

## ─── Account state ───────────────────────────────────────────────────────────

error-account-suspended = Your account has been suspended.
error-account-locked =
    Your account is temporarily locked due to failed login attempts. Try again later.
error-email-not-verified = Please verify your email address before continuing.
error-no-password-set = This account has no password set yet.
error-incorrect-current-password = Your current password is incorrect.
error-registration-closed = Registration is currently closed.
error-registration-conflict = An account with this email or username already exists.
error-email-taken = This email is already registered.
error-username-taken = This username is already taken.
error-invalid-or-expired-token = This link is invalid or has expired.
error-token-already-used = This link has already been used.
error-ban-expiry-must-be-future = The ban expiry date must be in the future.

## ─── Content: threads & posts ────────────────────────────────────────────────

error-thread-locked = This thread is locked.
error-category-closed = This category is closed to new posts.
error-edit-window-expired = The edit window for this has passed.
error-post-content-empty = Your post can't be empty.
error-post-content-too-long =
    Your post is too long. The limit is { $limit_kb } KB.
error-post-not-pending-approval = This post isn't waiting for approval.
error-cannot-react-to-own-post = You can't react to your own post.
error-reaction-exists = You've already reacted with this.
error-slug-taken = That name is already in use.
error-slug-reserved = That name is reserved and can't be used.
error-category-nesting-too-deep =
    Categories can only be nested { $max_depth } levels deep.
error-category-has-subcategories = This category still has subcategories and can't be deleted.
error-category-has-threads = This category still has threads and can't be deleted.
error-invalid-view-policy = That's not a valid visibility setting.
error-invalid-post-policy = That's not a valid posting setting.
error-invalid-reaction-kind = That's not a reaction you can use.
error-invalid-status = That's not a valid status.
error-invalid-smtp-port = The SMTP port must be a whole number between 1 and 65535.

## ─── Reports & moderation ────────────────────────────────────────────────────

error-report-target-required = A report must reference a post or a thread.
error-report-already-resolved = This report has already been handled.
error-reason-too-long =
    Please keep the reason to { $limit } characters or fewer.

## ─── Products & reviews ──────────────────────────────────────────────────────

error-product-already-reviewed =
    You've already reviewed this product. Edit your existing review instead.
error-product-name-required = Please enter a product name.
error-brand-name-required = Please enter a brand name.
error-material-name-required = Please enter a material name.
error-invalid-product-type = That's not a valid product type.
error-price-negative = Price can't be negative.
error-rating-out-of-range = Ratings must be between { $min } and { $max }.
error-thread-not-review = This thread isn't a product review.
error-product-has-reviews =
    This product can't be deleted while reviews still point at it. Archive it
    instead — that hides it from the catalogue and keeps those reviews intact.
error-product-media-limit =
    This product already has the maximum of { $limit } images. Remove one before
    adding another.

## ─── Uploads ─────────────────────────────────────────────────────────────────

error-upload-quota-exceeded = You've reached your daily upload limit. Try again tomorrow.
error-file-field-missing = No file was included in the upload.
error-image-field-missing = No image was included in the upload.
error-image-too-large = That image is too large. The limit is { $limit_mb } MB.
error-avatar-too-large = Your avatar must be { $limit_mb } MB or smaller.
error-cover-too-large = Your cover image must be { $limit_mb } MB or smaller.
error-logo-too-large = The logo must be { $limit_mb } MB or smaller.
error-logo-invalid-type = The logo must be a JPEG, PNG, WebP, or GIF image.
error-favicon-too-large = The favicon must be { $limit_kb } KB or smaller.
error-favicon-invalid-type = The favicon must be an ICO, PNG, GIF, or JPEG image. SVG isn't allowed.
error-hero-image-too-large = Each homepage hero image must be { $limit_mb } MB or smaller.
error-hero-image-invalid-type = A hero image must be a JPEG, PNG, WebP, or GIF image.
error-hero-tiles-full = The homepage hero holds { $limit } images. Remove one before adding another.
error-hero-image-unknown = That image isn't one of the uploaded hero images. Reload the page and try again.
error-hero-image-duplicate = The same image can't be used for two hero tiles.
error-hero-link-invalid = A hero link must start with / or with http:// or https://.
error-hero-link-too-long = A hero link must be { $limit } characters or fewer.
error-hero-caption-too-long = A hero caption must be { $limit } characters or fewer.

## ─── Preferences ─────────────────────────────────────────────────────────────

error-invalid-theme = Choose one of: auto, light, or dark.
error-invalid-font-size = Choose one of: small, medium, or large.
error-invalid-layout = Choose either compact or comfortable.
error-locale-not-enabled = That language isn't available on this site.

## ─── Webhooks ────────────────────────────────────────────────────────────────

error-webhook-url-required = Please enter a webhook URL.
error-webhook-url-invalid = That webhook URL isn't valid.
error-webhook-url-missing-host = The webhook URL needs a host name.

## ─── Plugins ─────────────────────────────────────────────────────────────────

error-plugin-already-installed = This plugin is already installed.
error-media-capability-not-granted = This plugin isn't allowed to upload media.
error-rpc-action-not-granted = This plugin action isn't available.
error-sql-not-allowed = This database operation isn't allowed.
error-multiple-sql-statements = Only one database statement can run at a time.
error-sql-quoted-identifiers-not-allowed = Quoted identifiers aren't allowed in plugin queries. Refer to your own tables by name, without quotes.
error-payload-too-large = That request was too large. If you're uploading a file, use the upload button rather than pasting the file's contents.
error-archive-unsafe-path = This package contains an unsafe file path and was rejected.
error-package-too-large = That package is too large. The limit is { $limit_mb } MB.
error-seed-data-unavailable = This build was compiled without the example data set, so it cannot be created. Finish setup with the box unticked — the forum works exactly the same, just without demo content.
error-invalid-granted-capabilities = The plugin capability settings aren't valid.
error-plugin-manifest-missing-meta = plugin.toml is missing its [meta] section.
error-plugin-manifest-missing-id = plugin.toml is missing meta.id.
error-plugin-manifest-missing-name = plugin.toml is missing meta.name.
error-plugin-manifest-missing-tier = plugin.toml is missing meta.tier.
error-plugin-manifest-missing-version = plugin.toml is missing meta.version.
error-plugin-manifest-missing-bundle-file =
    This script plugin's manifest is missing script.bundle_file.
error-slot-name-required = Please enter a slot name.

## ─── Roles ───────────────────────────────────────────────────────────────────

error-role-already-assigned = That role is already assigned to this user.

## ─── Field format & length ───────────────────────────────────────────────────

error-password-requirements =
    Your password must be at least 8 characters and include at least one digit
    and one special character.
error-invalid-username-format =
    Usernames must be 3–30 characters, using only letters, numbers, underscores,
    or hyphens.
error-display-name-length = Your display name must be 1–60 characters.
error-bio-too-long = Your bio must be 500 characters or fewer.
error-invalid-website-url =
    Enter a valid http:// or https:// address, up to 255 characters.
error-thread-title-length = Thread titles must be 5–255 characters.
error-invalid-trust-level = Choose one of: new, basic, member, regular, or leader.

## ─── Threads & posts (structure) ─────────────────────────────────────────────

error-parent-post-wrong-thread = That reply doesn't belong to this thread.
error-best-answer-wrong-thread = The best answer must be a post in this thread.

## ─── Products (structure) ────────────────────────────────────────────────────

error-price-range-inverted = The maximum price can't be lower than the minimum price.

## ─── Upload formats ──────────────────────────────────────────────────────────

error-image-invalid-type = Images must be JPEG, PNG, WebP, or GIF.
error-image-content-mismatch = That file isn't a supported image format.
error-avatar-invalid-type = Your avatar must be a JPEG, PNG, WebP, or GIF image.
error-cover-invalid-type = Your cover image must be a JPEG, PNG, WebP, or GIF image.
error-attachment-invalid-type = Attachments must be JPEG, PNG, WebP, or GIF images.
error-attachment-too-large = Attachments must be { $limit_mb } MB or smaller.
error-thumbnail-invalid-type = Thumbnails must be JPEG, PNG, WebP, or GIF images.
error-thumbnail-too-large = Thumbnails must be { $limit_mb } MB or smaller.
error-media-invalid-type = Media must be a JPEG, PNG, WebP, or GIF image.

## ─── Webhooks (validation) ───────────────────────────────────────────────────

error-webhook-events-required = Choose at least one event to send.
error-webhook-url-scheme = The webhook URL must start with http:// or https://.
error-webhook-url-private-address =
    The webhook URL can't point at a private, loopback, or link-local address.

## ─── Plugin packages ─────────────────────────────────────────────────────────

error-archive-path-traversal = This package contains a path traversal entry and was rejected.
error-archive-entry-escapes-dir =
    This package tries to write outside its own directory and was rejected.
error-archive-missing-manifest = This package has no plugin.toml at its root.
error-confirmation-slug-mismatch = The name you typed doesn't match the plugin's name.
error-plugin-id-length = A plugin ID must be between 1 and 256 characters.
error-plugin-id-charset =
    A plugin ID may contain only letters, numbers, dots, hyphens, and underscores.
error-plugin-id-dot-boundary = A plugin ID can't start or end with a dot.

## ─── Languages (admin) ───────────────────────────────────────────────────────

error-cannot-disable-last-locale =
    You can't disable the only language the site has. Add another first.
error-cannot-disable-default-locale =
    You can't disable the site's default language. Make another language the
    default first.
