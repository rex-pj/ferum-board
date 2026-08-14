# Shared UI strings — English (source catalog).
#
# Keys are kebab-case with a namespace prefix. Fluent message identifiers cannot
# contain dots, so `nav-search`, never `nav.search`.
#
# Templates call these as {{ t(k="nav-search") }}. Extra arguments become Fluent
# variables: {{ t(k="thread-replies", count=n) }}.

## ─── Language names ──────────────────────────────────────────────────────────
# Each language is named in its OWN language, not translated into the current
# one. A visitor looking for their language in a switcher is scanning for a word
# they recognise — "Tiếng Việt", not "Vietnamese" — and they may not be able to
# read the surrounding UI at all yet. These stay identical in every catalog.

language-name-en = English
language-name-vi = Tiếng Việt
language-name-ja = 日本語
language-name-de = Deutsch
language-name-fr = Français
language-name-es = Español
language-name-zh = 中文

## ─── Navigation & chrome ─────────────────────────────────────────────────────

nav-language = Language
nav-change-language = Change language
nav-search = Search

## ─── Thread & post ───────────────────────────────────────────────────────────

thread-replies =
    { $count ->
        [0] No replies
        [one] 1 reply
       *[other] { $count } replies
    }
thread-views =
    { $count ->
        [one] 1 view
       *[other] { $count } views
    }
thread-edited = edited

## ─── Preferences ─────────────────────────────────────────────────────────────

prefs-language = Language
prefs-language-help =
    Choose the language for menus and buttons. Threads and posts stay in the
    language they were written in.
prefs-language-auto = Match my browser

prefs-timezone = Time zone
prefs-timezone-help =
    Dates and times are shown in this zone. Leave it on your device's setting
    unless that setting is wrong, or you want times in your community's zone
    while you travel.
prefs-timezone-auto = Match my device

prefs-email-notifications = Email notifications
prefs-email-notifications-help =
    Choose what you also want by email. Everything else — reactions, follows —
    stays in your notification inbox only.
prefs-email-on-reply = Someone replies to a thread I started
prefs-email-on-mention = Someone mentions me
prefs-email-needs-verification =
    Your email address is not verified yet, so nothing will be sent until it is.

## ─── Unsubscribe ─────────────────────────────────────────────────────────────

ui-unsubscribe-title = Unsubscribe
ui-unsubscribe-done-title = You're unsubscribed
ui-unsubscribe-done-body =
    We won't email you about replies or mentions any more. You'll still see them
    in your notifications on the site.
ui-unsubscribe-failed-title = That link has expired
ui-unsubscribe-failed-body =
    Unsubscribe links stop working after a while. You can turn these emails off
    from your account settings instead — it takes a moment.
ui-unsubscribe-manage = Notification settings

## ─── Thread card ─────────────────────────────────────────────────────────────

thread-product-review = Product review
thread-review = Review
thread-last-reply-label = Last reply

## ─── Date & number formatting ────────────────────────────────────────────────
# These are FORMAT definitions, not sentences. Translators may reorder the
# fields freely — that is the point of expressing them as placeables instead of
# a strftime string, whose field order is fixed and whose month names are always
# English.
#
# $hour and $minute arrive already zero-padded.

# These are the SERVER-RENDERED forms, and the server renders in UTC — it does
# not know the reader's zone (see CLAUDE.md → Time and Timezones). Every use
# sits inside a <time data-rel|data-abs> that ferum-utils.js rewrites into the
# reader's own zone on load, so what these produce is the no-JavaScript
# fallback.
#
# The two that show a clock carry an explicit "UTC" for that reason: an
# unlabelled "23:30" is read as local and is wrong by the reader's offset,
# which near midnight is wrong by a whole day. The date-only forms are left
# unlabelled — "Aug 2026 UTC" for a join month is noise, and they are only ever
# the fallback for a relative time that JS replaces outright.
format-date = { $month } { $day }, { $year }
format-datetime = { $month } { $day }, { $year } { $hour }:{ $minute } UTC
format-daymonth = { $month } { $day }, { $hour }:{ $minute } UTC
format-monthyear = { $month } { $year }

# Single character. Anything longer is ignored and a comma is used instead.
format-thousands-separator = ,
# Alias so the separator reaches client-side JS, which can only see the `js-`
# namespace. A reference rather than a copy: the value is defined once above,
# and Ferum.formatNumber must group digits the same way the `thousands` Tera
# filter does or a price drawn by JS disagrees with the one beside it.
js-thousands-separator = { format-thousands-separator }

month-short-1 = Jan
month-short-2 = Feb
month-short-3 = Mar
month-short-4 = Apr
month-short-5 = May
month-short-6 = Jun
month-short-7 = Jul
month-short-8 = Aug
month-short-9 = Sep
month-short-10 = Oct
month-short-11 = Nov
month-short-12 = Dec

## ─── Error pages ─────────────────────────────────────────────────────────────

error-page-404-title = Page not found
error-page-404-lead = The page you're looking for doesn't exist.
error-page-404-body = The page you're looking for doesn't exist or may have been moved.
error-page-500-title = Something went wrong
error-page-500-lead = An unexpected error occurred.
error-page-500-body = An unexpected error occurred. Please try again in a moment.
error-page-reference = Reference:
action-go-home = Go home

## ─── Extracted page copy ─────────────────────────────────────────────────────
# Auto-extracted from the default theme's templates. Keys are derived from the
# English source text, so they read as a rough gloss of the string — that is
# intentional: it makes an untranslated key visible in the UI self-describing.

ui-1280-720-px-16-9-recommended = 1280 × 720 px (16:9) recommended
ui-about = About
ui-about-this-product = About this product
ui-account = Account
ui-account-settings = Account Settings
ui-account-status = Account Status
ui-actions = Actions
ui-add-a-product = Add a product
ui-add-select-for-review = Add & select for review
ui-admin = Admin
ui-all-categories = All Categories
ui-all-categories-2 = All categories
# The catalogue rail's "no category filter" row — the way back out of a category
# without losing the brand/material/query already picked.
ui-all-products = All products
ui-already-have-an-account = Already have an account?
ui-apply = Apply
ui-author = Author
ui-auto = Auto
ui-average-rating = Average rating
ui-awaiting-moderator-approval-only-visible-to = Awaiting moderator approval — only visible to you and staff
ui-back-to-feed = Back to feed
ui-back-to-sign-in = Back to sign in
ui-be-the-first = Be the first.
ui-best-answer = Best Answer
ui-bio = Bio
ui-bookmarks = Bookmarks
ui-brand = Brand
ui-brands = Brands
ui-browse = Browse
ui-browse-discussions = Browse discussions
ui-can-t-find-it-add-a = Can’t find it? Add a new product
ui-cancel = Cancel
ui-categories = Categories
ui-category = Category
ui-change = Change
ui-change-password = Change Password
ui-choose-a-category = Choose a category…
ui-choose-a-strong-password-for-your = Choose a strong password for your account.
ui-clear = Clear
ui-clear-filter = Clear filter
ui-close = Close
ui-comfortable = Comfortable
ui-community = Community
ui-compact = Compact
ui-confirm = Confirm
ui-confirm-new-password = Confirm new password
ui-confirm-password = Confirm password
ui-content = Content
ui-cover = Cover
ui-did-you-mean-one-of-these = Did you mean one of these?
ui-cover-image = Cover Image
ui-cover-image-2 = Cover image
ui-create-a-post = Create a post
ui-create-account = Create account
ui-create-one = Create one
ui-create-your-account = Create your account
ui-current-password = Current password
ui-dark = Dark
ui-archive = Archive
ui-delete = Delete
ui-delete-permanently = Delete permanently
ui-delete-product = Delete product
ui-description = Description
ui-denser-list = Denser list
ui-describe-why-this-post-violates-the = Describe why this post violates the rules…
ui-display-name = Display name
ui-don-t-have-an-account = Don't have an account?
ui-don-t-reuse-passwords-from-other = Don't reuse passwords from other sites
ui-drop-to-set-the-cover-image = Drop to set the cover image
ui-drop-to-set-thumbnail = Drop to set thumbnail
ui-edit = Edit
ui-edit-product = Edit product
ui-edit-profile = Edit profile
ui-edit-thread = Edit Thread
ui-email-address = Email address
ui-enter-your-email-and-we-ll = Enter your email and we'll send you a reset link.
ui-ferum-board = Ferum Board
ui-filter = Filter
ui-filter-by-category = Filter by category:
ui-follow = Follow
ui-followers = Followers
ui-following = Following
ui-font-size = Font Size
ui-footer-navigation = Footer navigation
ui-forgot-password = Forgot password?
ui-forgot-your-password = Forgot your password?
ui-forum = Forum
ui-from = From
ui-furniture = Furniture
ui-get-involved = Get Involved
ui-highest-rated = Highest rated
ui-home = Home
ui-home-feed = Home Feed
ui-i-have-actually-bought-or-used = I have actually bought or used this product
ui-inbox = Inbox
ui-invalid-email-or-password = Invalid email or password.
ui-invalid-or-missing-reset-token = Invalid or missing reset token.
ui-join-in = Join in
ui-join-the-community = Join the community
ui-join-the-conversation = Join the conversation
ui-jpeg-png-webp-gif-max-10 = JPEG · PNG · WebP · GIF · max 10 MB
ui-jump-to-best-answer = Jump to best answer
ui-large = Large
# Homepage sidebar panel: newest review per product, now that reviews are kept
# out of the discussion feed.
ui-latest-reviews = Latest reviews
# Fallback label for a review whose product row could not be loaded.
ui-a-product = A product
ui-leave-a-reply = Leave a reply
ui-leave-blank-to-use-username = Leave blank to use username
ui-letters-numbers-underscores-hyphens-cannot-be = Letters, numbers, underscores, hyphens. Cannot be changed later.
ui-light = Light
ui-loading = Loading…
ui-locked = Locked
ui-log-in = Log in
ui-log-in-to-review = Log in to review
ui-login = Login
ui-lowest-rated = Lowest rated
ui-manage-product = Manage product
ui-manage-watched-categories = Manage watched categories
ui-mark-all-read = Mark all read
ui-mark-best-answer = Mark best answer
ui-mark-read = Mark read
ui-material = Material
ui-materials = Materials
ui-max-n-characters = Max { $n } characters.
ui-medium = Medium
ui-member-since = Member since
ui-minimum-8-characters-including-a-digit = Minimum 8 characters, including a digit and a special character.
ui-mix-letters-numbers-symbols = Mix letters, numbers & symbols
ui-mobile-navigation = Mobile navigation
ui-mod = Mod
ui-moderator-actions = Moderator actions
ui-more-spacing = More spacing
ui-most-reviewed = Most reviewed
ui-move = Move
ui-move-thread = Move thread
ui-move-thread-2 = Move Thread
ui-moving-a-thread-to-a-different = Moving a thread to a different category is a moderator action.
ui-must-include-a-digit-and-a = Must include a digit and a special character.
ui-muted = Muted
ui-my-bookmarks = My Bookmarks
ui-my-reports = My Reports
ui-navigation-menu = Navigation menu
ui-new = New
ui-new-follower = New follower
ui-new-password = New password
ui-new-post = New post
ui-new-thread = New thread
ui-new-thread-2 = New Thread
ui-newest = Newest
ui-next = Next →
ui-next-2 = Next
ui-no-bookmarks-yet = No bookmarks yet
ui-no-brands-yet = No brands yet.
ui-no-categories-yet-an-admin-needs = No categories yet. An admin needs to create some first.
ui-no-cover-image-a-gradient-is = No cover image — a gradient is shown on your profile
ui-no-discussions-yet-be-the-first = No discussions yet. Be the first to start one!
ui-no-materials-yet = No materials yet.
ui-no-posts-found-for-the-tag = No posts found for the tag
ui-no-posts-have-been-marked-solved = No posts have been marked solved yet.
ui-no-product-images-yet = No product images yet
ui-no-results-found-for = No results found for "
ui-no-reviews-match-this-filter = No reviews match this filter.
ui-no-reviews-yet = No reviews yet
ui-no-reviews-yet-be-the-first = No reviews yet — be the first.
ui-no-threads-yet = No threads yet.
ui-none = — None —
ui-not-rated = Not rated
ui-nothing-left-unanswered-nice-work = Nothing left unanswered — nice work!
ui-notifications = Notifications
ui-open-menu = Open menu
ui-optional = (optional)
ui-optional-turns-this-post-into-a = (optional — turns this post into a scored review)
ui-origin = Origin
ui-overall = Overall
ui-page-navigation = Page navigation
ui-password = Password
ui-passwords-do-not-match = Passwords do not match.
ui-pending-approval = Pending approval
ui-pick-a-brand-to-see-all = Pick a brand to see all of its products.
ui-pick-a-material-to-see-every = Pick a material to see every product that uses it.
ui-pinned = Pinned
ui-photos = Photos
ui-please-enter-a-valid-url-starting = Please enter a valid URL starting with http:// or https://
ui-please-verify-your-email-address-before = Please verify your email address before continuing.
ui-post-actions = Post actions
ui-post-layout = Post Layout
ui-post-reply = Post Reply
ui-posted = Posted
ui-posting = Posting
ui-posts = Posts
ui-powered-by = Powered by
ui-preferences = Preferences
ui-press-enter-or-comma-to-add = Press Enter or comma to add. Max 5 tags.
ui-press-enter-or-comma-to-add-2 = Press Enter or comma to add. Up to 5 tags.
ui-previous = ← Previous
ui-previous-2 = Previous
ui-price-from = Price from (₫)
ui-price-on-request = Price on request
ui-price-range = Price range
ui-price-to = Price to (₫)
ui-product-images = Product images
ui-product-name = Product name
ui-product-review = Product review
ui-products = Products
ui-profile = Profile
ui-profile-settings = Profile settings
ui-rating-distribution = Rating distribution
ui-rating-headline-aria =
    { $count ->
        [0] { $score } out of 5 stars, no reviews yet
        [one] { $score } out of 5 stars, 1 review
       *[other] { $score } out of 5 stars, { $count } reviews
    }
ui-rating-histogram-row-aria =
    { $count ->
        [0] { $star } stars: no reviews
        [one] { $star } stars: 1 review
       *[other] { $star } stars: { $count } reviews
    }
ui-reach-member-trust-level-or-get = Reach Member trust level (or get verified) to attach a thumbnail image.
ui-reach-member-trust-level-or-get-2 = Reach Member trust level (or get verified) to attach a cover image.
ui-reason = Reason
ui-recent-discussions = Recent Discussions
ui-recommended-1200-400-px-jpeg-png = Recommended: 1200×400 px, JPEG/PNG/WebP, max 8 MB.
ui-recommended-1280-720-px-16-9 = Recommended 1280 × 720 px (16:9)
ui-register = Register
ui-registration-is-currently-closed-please-contact = Registration is currently closed. Please contact an administrator.
ui-remove = Remove
ui-remove-tag = Remove tag
ui-remove-thumbnail = Remove thumbnail
ui-replies = Replies
ui-reply = Reply
ui-reply-to-thread = Reply to thread
ui-report = Report
ui-report-post = Report Post
ui-reporting-post-by = Reporting post by
ui-reports-you-ve-filed = Reports You've Filed
ui-request-a-new-link = Request a new link
ui-request-a-new-one = Request a new one.
ui-resend-verification-email = Resend verification email
ui-review = Review
ui-review-summary = Review summary
ui-reviewing-a-product = Reviewing a product?
ui-reviews = Reviews
ui-role = Role
ui-room = Room
ui-save = Save
ui-save-preferences = Save Preferences
ui-save-profile = Save Profile
ui-save-threads-for-later-by-clicking = Save threads for later by clicking the bookmark icon on any thread page.
ui-saved = Saved
ui-saved-threads = Saved Threads
ui-saved-threads-2 = Saved threads
ui-scores-by-dimension = Scores by dimension
ui-search = Search
ui-search-all-categories = Search all categories
ui-search-for-a-product-to-review = Search for a product to review… (leave empty for a regular post)
ui-search-is-temporarily-unavailable-please-try = Search is temporarily unavailable. Please try again in a moment.
ui-search-products = Search products…
ui-search-tips = Search Tips
# Site-wide search covers the catalogue as well as the forum; the placeholder
# has to say so, or nobody thinks to look for a product here.
ui-search-placeholder-everything = Search products and discussions…
ui-search-result-types = Result types
ui-search-tab-all = All
ui-search-tab-products = Products
ui-search-tab-discussions = Discussions
ui-see-all-n-products =
    { $count ->
        [one] See the 1 product
       *[other] See all { $count } products
    }
ui-see-n-products-instead =
    { $count ->
        [one] See 1 matching product
       *[other] See { $count } matching products
    }
ui-see-n-discussions-instead =
    { $count ->
        [one] See 1 matching discussion
       *[other] See { $count } matching discussions
    }
ui-tip-searches-products-and-threads = Searches product names, brands, thread titles and post replies
ui-tip-accents-optional = Vietnamese accents are optional — “ghe an” finds “ghế ăn”
ui-tip-filter-products-by-brand = On the Products tab, narrow by type, brand or material
ui-sort-relevance = Best match
ui-sort-most-replies = Most replies
# The category filter spans both kinds; the product facets do not. Saying so
# next to the control is what stops a reader thinking their brand filter also
# shrank the discussion count.
# The catalogue's own tree (Sofa, Ghế, Bàn …), not the forum's. Named for the
# thing it classifies so it cannot be mistaken for the discussion categories.
ui-product-category = Category
ui-too-many-filters = No match for this combination of filters.
ui-drop-product-filters =
    { $count ->
        [one] Without the product filters: 1 product
       *[other] Without the product filters: { $count } products
    }
ui-drop-category-filter =
    { $count ->
        [one] Without the category: 1 result
       *[other] Without the category: { $count } results
    }
ui-security = Security
ui-see-warning-details-in-your-notifications = See warning details in your notifications
ui-select-a-category = Select a category…
ui-select-if-known = (select if known)
ui-select-the-target-category-for-this = Select the target category for this thread.
ui-set-new-password = Set new password
ui-shorter-queries-return-more-results = Shorter queries return more results
# Collapses the homepage product shelf back to a single row. Static, unlike its
# "show more" counterpart (js-show-more-products), which carries a live count.
ui-show-less = Show less
ui-show-password = Show password
ui-sign-in = Sign in
ui-sign-in-to-watch-this-category = Sign in to watch this category
ui-sign-in-to-your-account-to = Sign in to your account to continue.
ui-site-navigation = Site navigation
ui-small = Small
ui-solved = Solved
ui-someone-started-following-you = Someone started following you
ui-sort = Sort
ui-sort-reviews = Sort reviews
ui-start-the-first-thread = Start the first thread
ui-started-following-you = started following you
ui-strongest = Strongest:
ui-style = Style
ui-subcategories = Subcategories
ui-submit-report = Submit Report
ui-suggest-a-product = Suggest a product
ui-summary = Summary
ui-tags = Tags
ui-that-verification-link-is-invalid-or = That verification link is invalid or has expired.
ui-the-product-becomes-public-once-an = The product becomes public once an admin approves it. Your review is saved right away either way.
ui-theme = Theme
ui-this-post-has-been-deleted = This post has been deleted.
ui-this-thread-is-locked-and-no = This thread is locked and no longer accepts new replies.
ui-thread-info = Thread Info
ui-threads = Threads
ui-thumbnail = Thumbnail
ui-thumbnail-preview = Thumbnail preview
ui-tips-for-a-strong-password = Tips for a strong password
ui-title = Title
ui-to = To
ui-toggle-dark-light-mode = Toggle dark/light mode
ui-toggle-theme = Toggle theme
ui-top-rated = Top rated
ui-top-rated-products = Top rated products
# Heading the product shelf falls back to when too few products carry enough
# reviews to justify calling anything "top rated". Recency is a claim the
# catalogue can always back, so the shelf keeps showing products either way.
ui-newest-products = Newest products
ui-total-shown = Total shown
ui-trending-now = Trending now
ui-trust-level = Trust level
ui-trust-score = Trust score
ui-type = Type
ui-type-a-material-name-then-pick = Type a material name, then pick it to add.
ui-type-to-search-materials = Type to search materials…
ui-unanswered = Unanswered
ui-unread = Unread
ui-uploading = Uploading
ui-uploading-2 = Uploading…
ui-use-at-least-8-characters = Use at least 8 characters
ui-use-specific-keywords = Use specific keywords
ui-use-the-category-filter-to-narrow = Use the category filter to narrow results
ui-username = Username
# Brands only: `brands.is_verified`, which a curator sets from /admin/products.
# That one really is checked by a human, unlike the review badge below.
ui-verified = Verified

# The reviewer ticked "I bought or used this" on the review form. Nothing
# checks it, so the wording must not imply that anything did — see the badge in
# catalog/product.html. When a real verification mechanism exists it gets its
# own key rather than quietly changing the meaning of this one.
ui-self-reported-purchase = Says they bought it
ui-self-reported-purchase-hint =
    The reviewer says they bought or used this. We have not verified it.
ui-self-reported-purchases-only = Self-reported buyers only
ui-view-all = View all
ui-view-all-discussions = View all discussions
ui-view-product = View product
ui-view-public-profile = View public profile
ui-view-thread = View thread
ui-view-your-review = View your review
ui-views = Views
ui-warnings = Warnings
ui-watch = Watch
ui-watching = Watching
ui-watching-muted = Watching & Muted
ui-weakest = Weakest:
ui-website = Website
ui-welcome-back = Welcome back
ui-write-a-clear-specific-title = Write a clear, specific title
ui-write-a-review = Write a review
ui-write-your-post-in-markdown = Write your post in Markdown…
ui-you-have-already-reviewed-this-product = You have already reviewed this product. Edit your existing review to update it.
ui-you-haven-t-reported-anything-yet = You haven't reported anything yet.
ui-you-re-all-caught-up-no = You're all caught up — no notifications.
ui-you-received-a-warning = You received a warning
ui-your-account-does-not-yet-have = Your account does not yet have permission to reply in this category. Verify your email or gain more activity to unlock posting.
ui-your-account-has-been-suspended = Your account has been suspended.
ui-your-account-is-temporarily-locked-due = Your account is temporarily locked due to failed login attempts. Try again later.
ui-your-email-has-been-verified-you = Your email has been verified. You can now sign in.
ui-your-rating = Your rating
ui-your-submission-pending = This product is awaiting moderator approval — you can still add or remove its photos until then.
ui-your-submission-published = You submitted this product, now shared catalogue content — ask a moderator if it needs changing.

## ─── Merged sentences ────────────────────────────────────────────────────────
# These replace strings that auto-extraction had split across inline markup.
# A sentence broken into fragments cannot be translated: word order differs by
# language, so the pieces would have to be reassembled in an order the template
# fixes in place. Each of these is one whole sentence instead.

ui-watching-empty =
    You're not watching any categories yet. Open a category and choose Watch to
    follow it.
ui-muting-empty =
    You're not muting any categories. Open a category and choose Mute to hide it.
ui-rating-hint = Tap the stars to score. Only the overall rating is required.
ui-public-once-approved = Will be public once approved
ui-sign-in-to-join = Sign in to join the conversation
ui-drop-or-browse = Drop a file here, or click to browse
ui-drop-or-choose-image = Drag and drop an image here, or click to choose one

## ─── Page titles & meta descriptions ─────────────────────────────────────────
# The <title> and <meta description> of every page. Kept as whole sentences
# rather than glued-together fragments so a translator can reorder them.

ui-title-forgot-password = Forgot password
ui-desc-catalog = Browse furniture, materials and rooms on { $site }.
ui-desc-brands = A directory of furniture brands on { $site }.
ui-desc-materials = Look up furniture materials and browse products by material on { $site }.
ui-desc-thread = { $title } by { $author }
ui-breadcrumb = Breadcrumb

## ─── Countable labels ────────────────────────────────────────────────────────
# The count itself stays in the template (it is often wrapped in <strong>), so
# these carry only the noun and its plural form.

ui-products-label =
    { $count ->
        [one] product
       *[other] products
    }
ui-threads-label =
    { $count ->
        [one] thread
       *[other] threads
    }
ui-reviews-label =
    { $count ->
        [one] review
       *[other] reviews
    }
ui-results-label =
    { $count ->
        [one] result for
       *[other] results for
    }
ui-page-x-of-y = page { $page } of { $total }

## ─── Account status ──────────────────────────────────────────────────────────

ui-account-suspended = Your account is suspended.
# Label only — the template appends ": " and the expiry as its own <time>
# element, so the client can show it in the reader's timezone. It replaced a
# `{ $date }` placeable, which produced plain text stuck in UTC.
ui-account-suspended-until-label = Suspended until
ui-change-cover = Change cover
ui-upload-cover = Upload cover
ui-change-photo = Change photo
ui-upload-photo = Upload photo

## ─── Notification lines ──────────────────────────────────────────────────────
# Each notification renders a headline (no actor known) and a body line that
# follows the actor's name, which is why several read as sentence fragments.

ui-notification = Notification
ui-issued-a-warning = issued a warning
ui-a-moderator-issued-a-warning = A moderator issued a warning
ui-new-reply-on-your-thread = New reply on your thread
ui-you-were-mentioned = You were mentioned
ui-someone-reacted-to-your-post = Someone reacted to your post
ui-best-answer-marked = Best answer marked
ui-replied-to-your-thread = replied to your thread
ui-someone-replied-to-your-thread = Someone replied to your thread
ui-mentioned-you-in-a-post = mentioned you in a post
ui-you-were-mentioned-in-a-post = You were mentioned in a post
ui-reacted = reacted
ui-someone-reacted = Someone reacted
ui-to-your-post = to your post
ui-marked-your-post-as-best-answer = marked your post as best answer
ui-your-post-was-marked-as-best-answer = Your post was marked as best answer

## ─── Compose & moderation actions ────────────────────────────────────────────

ui-publish = Publish
ui-pin = Pin
ui-unpin = Unpin
ui-lock = Lock
ui-unlock = Unlock

## ─── Review dimensions ───────────────────────────────────────────────────────

ui-durability = Durability
ui-comfort = Comfort
ui-aesthetics = Aesthetics
ui-value-for-money = Value for money
ui-value = Value
ui-out-of-5 = out of 5

## ─── Category policy labels ──────────────────────────────────────────────────

ui-policy-members = Members
ui-policy-trusted = Trusted
ui-policy-staff-only = Staff only
ui-policy-closed = Closed

## ─── Product type & material category labels ─────────────────────────────────

ui-product-type-furniture = Furniture
ui-product-type-material = Material
ui-product-type-room = Room
ui-material-wood-natural = Solid wood
ui-material-wood-engineered = Engineered wood
ui-material-rattan-bamboo = Rattan & bamboo
ui-material-metal = Metal
ui-material-fabric = Fabric
ui-material-leather = Leather
ui-material-stone = Stone
ui-material-glass = Glass
ui-material-plastic = Plastic
ui-material-other = Other

## ─── Empty states & invitations ──────────────────────────────────────────────

ui-join-today = Join { $site } today.
ui-join-the-community-and-start = Join the community and start discussing.
ui-join-the-conversation-and-share = Join the conversation and share your thoughts.
ui-personalised-from-categories = Personalised from { $count } categories
ui-no-unanswered-threads = No unanswered threads — great work!
ui-no-solved-threads-yet = No solved threads in this category yet.
ui-no-threads-in-this-category = No threads in this category yet.
ui-no-products-found-for-query = No products found for “{ $query }”.
ui-no-products-match-this-filter = No products match this filter.
ui-suggest-a-product-named = Suggest a product “{ $query }”
ui-no-reviews-yet-period = No reviews yet.
ui-in-parent-category = In { $category }
ui-related = Related
ui-in-this-category = in this category

## ─── Ferum Review theme ──────────────────────────────────────────────────────
# Copy unique to the ferum-review theme — the default theme has no equivalent
# strings, so these have no counterpart elsewhere in this file. See
# frontend/themes/ferum-review/templates/home.html.
#
# The masthead's own copy is NOT here: it moved to the `home-hero` plugin, whose
# text is operator-entered per locale in the plugin's config rather than
# translated from a catalog. See examples/plugins/home-hero/README.md.

ui-discussion-categories = Discussion categories
ui-browse-by-type = Browse by type

## ─── Client-side messages ────────────────────────────────────────────────────
# Rendered by JavaScript rather than the server, so they reach the browser via
# the `window.Ferum.i18n` dictionary that `base.html` emits. Only these keys are
# shipped to the client — the full catalog stays on the server.

# Relative timestamps, rendered by Ferum.timeAgo. Compact by design — they sit
# under avatars and in list rows. The absolute fallback past ~30 days goes
# through Ferum.formatDate, which uses <html lang> rather than a key.
js-time-just-now = just now
js-time-minutes = { $n }m ago
js-time-hours = { $n }h ago
js-time-days = { $n }d ago
js-time-weeks = { $n }w ago

js-network-error = Network error. Please try again.
js-failed-to-load = Failed to load.
js-passwords-do-not-match = Passwords do not match.
js-password-requirements = Password must be at least 8 characters and include a digit and a special character.
js-password-missing-digit-special = Must include a digit and a special character.
js-account-created = Account created! Redirecting to sign in…
js-password-updated = Password updated! Redirecting to sign in…
js-reset-link-sent = If an account with that email exists, a reset link has been sent.
js-verify-link-sent = If that address needs verifying, a new link is on its way.
js-reply-pending-approval = Your reply has been submitted and is awaiting moderator approval.
js-report-submitted = Report submitted. Thank you.
js-provide-a-reason = Please provide a reason.
js-write-something-first = Please write something before posting.
js-select-target-category = Please select a target category.
js-enter-product-name = Please enter a product name.
js-no-products-found = No products found.
js-could-not-mark-read = Could not mark as read. Please try again.
js-could-not-mark-all-read = Could not mark all as read. Please try again.
js-failed-unwatch = Failed to unwatch category. Please try again.
js-failed-unmute = Failed to unmute category. Please try again.
js-could-not-change-language = Could not change language.
js-pending-approval = Pending approval
js-resend-email = Resend email

# Confirm dialog + password-strength meter (ferum-utils.js)
js-confirm = Confirm
js-type-to-confirm = Type { $code } to confirm:
js-pw-very-weak = Very weak
js-pw-weak = Weak
js-pw-fair = Fair
js-pw-strong = Strong
js-pw-very-strong = Very strong

# Account page
js-invalid-website-url = Please enter a valid URL starting with http:// or https://
js-profile-saved = Profile saved.
js-failed-to-save = Failed to save.
js-avatar-invalid-type = Avatar must be a JPEG, PNG, GIF, or WebP image.
js-avatar-too-large = Avatar must be under 5 MB.
js-avatar-updated = Avatar updated.
js-avatar-removed = Avatar removed.
js-failed-remove-avatar = Failed to remove avatar.
js-cover-invalid-type = Cover must be a JPEG, PNG, GIF, or WebP image.
js-cover-too-large = Cover image must be under 8 MB.
js-cover-updated = Cover updated.
js-cover-removed = Cover removed.
js-failed-remove-cover = Failed to remove cover.
js-upload-failed = Upload failed.
js-password-changed = Password changed successfully.
js-failed-change-password = Failed to change password.
js-preferences-saved = Preferences saved.
js-failed-save-preferences = Failed to save preferences.

# Auth pages
js-invalid-credentials = Invalid email or password.
js-registration-failed = Registration failed. Please try again.
js-resend-in = Resend in { $seconds }s
js-reset-failed = Reset failed. The link may have expired.

# Compose (new thread / edit thread)
js-thumbnail-invalid-type = Thumbnail must be a JPEG, PNG, GIF, or WebP image.
js-thumbnail-too-large = Thumbnail must be under 10 MB.
js-failed-save-changes = Failed to save changes.
js-could-not-load-materials = Could not load the material list.
js-no-materials-in-catalog = No materials in the catalog yet.
js-could-not-add-product = Could not add the product.
js-delete-product-confirm = Delete
js-material-links = Material links
js-photos = Photos
js-reviews = Reviews
js-product-delete-blocked =
    This product has { $count } review(s). Deleting it would leave them reviewing
    nothing, so permanent deletion is blocked. Archive it instead — it disappears
    from the catalogue and every review stays intact.
js-product-delete-permanent-warning =
    Nothing references this product, so it can be removed for good. Its images are
    released from storage at the same time. This cannot be undone.
js-product-added-photos-failed =
    The product was added, but some photos couldn't be uploaded. You can add them
    later once it's approved.
js-remove-photo = Remove photo
js-remove-material = Remove material
js-no-materials-found = No materials match that.
js-price-from-exceeds-to = "Price from" cannot be greater than "price to".
js-use-this-product = Use this one
js-give-overall-score = Please give the product at least an Overall score.
js-could-not-publish = Could not publish. Please try again.
js-write-a-review = Write a review
js-new-post = New post
js-publish = Publish
js-publish-review = Publish review

# Product type & material category labels, mirroring the Tera macros
js-product-type-furniture = { ui-product-type-furniture }
js-product-type-material = { ui-product-type-material }
js-product-type-room = { ui-product-type-room }
js-material-wood-natural = { ui-material-wood-natural }
js-material-wood-engineered = { ui-material-wood-engineered }
js-material-rattan-bamboo = { ui-material-rattan-bamboo }
js-material-metal = { ui-material-metal }
js-material-fabric = { ui-material-fabric }
js-material-leather = { ui-material-leather }
js-material-stone = { ui-material-stone }
js-material-glass = { ui-material-glass }
js-material-plastic = { ui-material-plastic }
js-material-other = { ui-material-other }

# Profile page
js-nobody-here-yet = Nobody here yet.
js-no-posts-yet = No posts yet.
js-view-thread = View thread

# Thread page
js-failed-delete-post = Failed to delete post.
js-failed-submit-report = Failed to submit report.

# Post composer widget
js-composer-write = Write
js-composer-preview = Preview
js-composer-placeholder = Write your reply in Markdown…
js-composer-nothing-to-preview = Nothing to preview.
js-composer-hint = Markdown supported · Ctrl+B / Ctrl+I / Ctrl+K
js-composer-toolbar = Formatting toolbar
js-composer-bold = Bold (Ctrl+B)
js-composer-italic = Italic (Ctrl+I)
js-composer-strikethrough = Strikethrough
js-composer-heading-1 = Heading 1
js-composer-heading-2 = Heading 2
js-composer-quote = Quote
js-composer-code-inline = Code (inline)
js-composer-code-block = Code block
js-composer-link = Link (Ctrl+K)
js-composer-ul = Unordered list
js-composer-ol = Ordered list
js-composer-attach-image = Attach image
js-failed-upload-image = Failed to upload image.

# Other widgets
js-bookmark = Bookmark
js-bookmarked = Bookmarked
js-bookmark-this-thread = Bookmark this thread
js-remove-bookmark = Remove bookmark
js-clear = Clear
js-clear-selection = Clear selection
js-type-to-search = Type to search…
js-search = Search
js-no-matches = No matches

# Homepage product shelf. The count is how many cards the current breakpoint
# hides, so it is measured in the browser rather than rendered by the server.
js-show-more-products = Show { $count } more

# Thread page confirmations & failures
js-delete-post-title = Delete Post
js-delete-post-body = Delete this post? This cannot be undone.
js-delete = Delete
js-failed-post-reply = Failed to post reply.
js-mark-best-answer-title = Mark Best Answer
js-mark-best-answer-body = Mark this post as the best answer?
js-mark-as-best = Mark as best
js-failed-mark-best-answer = Failed to mark best answer.
js-failed-move-thread = Failed to move thread.
js-delete-thread-title = Delete Thread
js-delete-thread-body = Delete this entire thread? This cannot be undone.
js-delete-thread-ok = Delete thread
js-failed-delete-thread = Failed to delete thread.
js-failed-thread-action = Could not update the thread. Please try again.

# Widget-only strings
js-notifications = Notifications
js-unread-count = { $count } unread
js-loading = Loading…
js-scroll-for-more = Scroll for more…
js-cannot-react-own-post = You can't react to your own post.
js-react-trust-insufficient = Your account needs to be verified to react.
js-account-suspended = Your account is suspended.

## ─── Search filters ──────────────────────────────────────────────────────────
# Each facet control's default option is its own NAME, not "All …". Four
# dropdowns that all begin with the same word cannot be scanned — the eye has to
# read to the end of each to tell them apart.
ui-filter-by = Filter by:
ui-active-filters = Active filters
ui-clear-all-filters = Clear all

## ─── Thread feed: sort tabs + filter chips ───────────────────────────────────
# Two axes, deliberately labelled apart. The sort tabs reorder the list; the
# filter chips remove rows from it. They used to be one strip of five tabs with
# identical affordances, where three reordered and two made threads disappear.
#
# Every label states what the query actually does. "Most discussed" is not
# "Hottest" because the ordering carries no time decay — it ranks all-time reply
# volume, and a label promising trending content over a query that cannot
# express it is a bug that no test can catch.
ui-discussions = Discussions
ui-sort-by = Sort by
ui-sort-activity = Recent activity
ui-sort-activity-hint = Threads with the newest replies
ui-sort-newest = Newly posted
ui-sort-newest-hint = Threads created most recently
ui-sort-most-discussed = Most discussed
ui-sort-most-discussed-hint = Threads with the most replies, all time
ui-filter-label = Filter
ui-filter-unanswered-hint = Only open threads nobody has replied to
ui-filter-solved-hint = Only threads with an accepted best answer
