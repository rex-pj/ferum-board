# Vietnamese UI strings.
#
# Only keys that differ from English need to be here — anything missing falls
# back to `en` via the locale chain, so a partial translation still renders a
# working site.
#
# Language names are intentionally NOT translated: each language is named in its
# own tongue so a visitor can find theirs without reading the surrounding UI.

language-name-en = English
language-name-vi = Tiếng Việt
language-name-ja = 日本語
language-name-de = Deutsch
language-name-fr = Français
language-name-es = Español
language-name-zh = 中文

## ─── Điều hướng ──────────────────────────────────────────────────────────────

nav-language = Ngôn ngữ
nav-change-language = Đổi ngôn ngữ
nav-search = Tìm kiếm

## ─── Chủ đề & bài viết ───────────────────────────────────────────────────────

# Tiếng Việt không biến đổi theo số lượng, nên mọi nhánh số nhiều dùng chung
# một dạng. Vẫn giữ cấu trúc { $count -> } để catalog khớp với bản gốc.
thread-replies =
    { $count ->
        [0] Chưa có trả lời
       *[other] { $count } trả lời
    }
thread-views =
    { $count ->
       *[other] { $count } lượt xem
    }
thread-edited = đã sửa

## ─── Tùy chọn ────────────────────────────────────────────────────────────────

prefs-language = Ngôn ngữ
prefs-language-help =
    Chọn ngôn ngữ cho menu và nút bấm. Chủ đề và bài viết vẫn giữ nguyên ngôn ngữ
    người viết đã dùng.
prefs-language-auto = Theo trình duyệt

## ─── Thẻ chủ đề ──────────────────────────────────────────────────────────────

thread-product-review = Đánh giá sản phẩm
thread-review = Đánh giá
thread-last-reply-label = Trả lời gần nhất

## ─── Định dạng ngày & số ─────────────────────────────────────────────────────
# Tiếng Việt viết ngày trước tháng, và dùng dấu chấm để phân tách hàng nghìn.

format-date = { $day } { $month }, { $year }
format-datetime = { $day } { $month }, { $year } { $hour }:{ $minute }
format-daymonth = { $day } { $month }, { $hour }:{ $minute }
format-monthyear = { $month } { $year }

format-thousands-separator = .

month-short-1 = thg 1
month-short-2 = thg 2
month-short-3 = thg 3
month-short-4 = thg 4
month-short-5 = thg 5
month-short-6 = thg 6
month-short-7 = thg 7
month-short-8 = thg 8
month-short-9 = thg 9
month-short-10 = thg 10
month-short-11 = thg 11
month-short-12 = thg 12

## ─── Trang lỗi ───────────────────────────────────────────────────────────────

error-page-404-title = Không tìm thấy trang
error-page-404-lead = Trang bạn tìm không tồn tại.
error-page-404-body = Trang bạn tìm không tồn tại hoặc đã được chuyển đi.
error-page-500-title = Đã có lỗi xảy ra
error-page-500-lead = Xảy ra lỗi ngoài dự kiến.
error-page-500-body = Xảy ra lỗi ngoài dự kiến. Vui lòng thử lại sau giây lát.
error-page-reference = Mã tham chiếu:
action-go-home = Về trang chủ

## ─── Thông báo phía trình duyệt ──────────────────────────────────────────────

js-network-error = Lỗi kết nối. Vui lòng thử lại.
js-failed-to-load = Không tải được.
js-passwords-do-not-match = Mật khẩu nhập lại không khớp.
js-password-requirements = Mật khẩu phải có ít nhất 8 ký tự, gồm một chữ số và một ký tự đặc biệt.
js-password-missing-digit-special = Phải có ít nhất một chữ số và một ký tự đặc biệt.
js-account-created = Đã tạo tài khoản! Đang chuyển tới trang đăng nhập…
js-password-updated = Đã cập nhật mật khẩu! Đang chuyển tới trang đăng nhập…
js-reset-link-sent = Nếu email này đã đăng ký, liên kết đặt lại mật khẩu đã được gửi đi.
js-verify-link-sent = Nếu địa chỉ này cần xác minh, liên kết mới đang được gửi đi.
js-reply-pending-approval = Trả lời của bạn đã được gửi và đang chờ kiểm duyệt.
js-report-submitted = Đã gửi báo cáo. Cảm ơn bạn.
js-provide-a-reason = Vui lòng nhập lý do.
js-write-something-first = Vui lòng nhập nội dung trước khi đăng.
js-select-target-category = Vui lòng chọn chuyên mục đích.
js-enter-product-name = Vui lòng nhập tên sản phẩm.
js-no-products-found = Không tìm thấy sản phẩm nào.
js-could-not-mark-read = Không đánh dấu đã đọc được. Vui lòng thử lại.
js-could-not-mark-all-read = Không đánh dấu tất cả đã đọc được. Vui lòng thử lại.
js-failed-unwatch = Không bỏ theo dõi chuyên mục được. Vui lòng thử lại.
js-failed-unmute = Không bỏ tắt tiếng chuyên mục được. Vui lòng thử lại.
js-could-not-change-language = Không đổi được ngôn ngữ.
js-pending-approval = Chờ duyệt
js-resend-email = Gửi lại email

## Điều hướng & khung trang
ui-home = Trang chủ
ui-home-feed = Bảng tin
ui-forum = Diễn đàn
ui-from = Từ
ui-search = Tìm kiếm
# Tìm kiếm toàn site bao gồm cả danh mục sản phẩm lẫn diễn đàn; phần gợi ý
# trong ô tìm kiếm phải nói rõ điều đó, nếu không sẽ không ai nghĩ tới việc
# tìm sản phẩm ở đây.
ui-search-placeholder-everything = Tìm sản phẩm, thảo luận…
ui-categories = Chuyên mục
ui-category = Chuyên mục
ui-account = Tài khoản
ui-account-settings = Cài đặt tài khoản
ui-notifications = Thông báo
ui-bookmarks = Đã lưu
ui-inbox = Hộp thư
ui-profile = Trang cá nhân
ui-admin = Quản trị
ui-mod = Kiểm duyệt
ui-community = Cộng đồng
ui-browse = Duyệt
ui-browse-discussions = Duyệt các chủ đề
ui-open-menu = Mở menu
ui-navigation-menu = Menu điều hướng
ui-mobile-navigation = Điều hướng trên di động
ui-site-navigation = Điều hướng trang
ui-footer-navigation = Điều hướng chân trang
ui-page-navigation = Điều hướng trang
ui-toggle-theme = Đổi giao diện
ui-toggle-dark-light-mode = Chuyển chế độ sáng/tối
ui-powered-by = Vận hành bởi

## Hành động chung
ui-cancel = Hủy
ui-confirm = Xác nhận
ui-archive = Lưu trữ
ui-delete = Xóa
ui-delete-permanently = Xóa vĩnh viễn
ui-delete-product = Xóa sản phẩm
ui-description = Mô tả
ui-edit = Sửa
ui-edit-product = Sửa sản phẩm
ui-apply = Áp dụng
ui-clear = Xóa lọc
ui-clear-filter = Bỏ bộ lọc
ui-filter = Lọc
ui-sort = Sắp xếp
ui-save = Lưu
ui-loading = Đang tải…
ui-view-all = Xem tất cả
ui-new = Mới
ui-next = Sau
ui-previous = Trước

## Chủ đề & bài viết
ui-new-thread = Tạo chủ đề
ui-create-a-post = Viết bài
ui-leave-a-reply = Viết trả lời
ui-reply = Trả lời
ui-replies = Trả lời
ui-views = Lượt xem
ui-posts = Bài viết
ui-threads = Chủ đề
ui-author = Tác giả
ui-posted = Đã đăng
ui-pinned = Đã ghim
ui-locked = Đã khóa
ui-solved = Đã giải quyết
ui-best-answer = Câu trả lời hay nhất
ui-mark-best-answer = Chọn câu trả lời hay nhất
ui-jump-to-best-answer = Tới câu trả lời hay nhất
ui-latest-discussions = Chủ đề mới nhất
# Panel sidebar trang chủ: đánh giá mới nhất mỗi sản phẩm, sau khi review đã
# được tách khỏi feed thảo luận.
ui-latest-reviews = Đánh giá mới nhất
# Nhãn dự phòng khi không tải được sản phẩm của một bài đánh giá.
ui-a-product = Một sản phẩm
ui-recent-discussions = Chủ đề gần đây
ui-trending-now = Đang được quan tâm
ui-newest = Mới nhất
ui-hottest = Sôi nổi nhất
ui-unread = Chưa đọc
ui-tags = Thẻ
ui-title = Tiêu đề
ui-content = Nội dung
ui-thumbnail = Ảnh đại diện chủ đề
ui-back-to-feed = Về bảng tin
ui-view-thread = Xem chủ đề
ui-no-threads-yet = Chưa có chủ đề nào.
ui-mark-read = Đánh dấu đã đọc
ui-mark-all-read = Đánh dấu tất cả đã đọc
ui-report-post = Báo cáo bài viết
ui-reason = Lý do
ui-edit-thread = Sửa chủ đề
ui-move-thread = Chuyển chủ đề

## Đăng nhập & đăng ký
ui-log-in = Đăng nhập
ui-login = Đăng nhập
ui-sign-in = Đăng nhập
ui-register = Đăng ký
ui-create-account = Tạo tài khoản
ui-create-your-account = Tạo tài khoản của bạn
ui-create-one = Tạo tài khoản
ui-welcome-back = Chào mừng trở lại
ui-email-address = Địa chỉ email
ui-password = Mật khẩu
ui-username = Tên đăng nhập
ui-confirm-password = Nhập lại mật khẩu
ui-confirm-new-password = Nhập lại mật khẩu mới
ui-current-password = Mật khẩu hiện tại
ui-new-password = Mật khẩu mới
ui-change-password = Đổi mật khẩu
ui-forgot-password = Quên mật khẩu?
ui-forgot-your-password = Quên mật khẩu?
ui-back-to-sign-in = Về trang đăng nhập
ui-set-new-password = Đặt mật khẩu mới
# Thu kệ sản phẩm trang chủ về một hàng. Chuỗi tĩnh, khác với "xem thêm"
# (js-show-more-products) vì bên kia có số đếm động.
ui-show-less = Thu gọn
ui-show-password = Hiện mật khẩu
ui-already-have-an-account = Đã có tài khoản?
ui-don-t-have-an-account = Chưa có tài khoản?
ui-invalid-email-or-password = Email hoặc mật khẩu không đúng.
ui-join-the-conversation = Tham gia thảo luận
ui-join-the-community = Tham gia cộng đồng
ui-sign-in-to-join = Đăng nhập để tham gia thảo luận

## Tài khoản & tùy chọn
ui-display-name = Tên hiển thị
ui-bio = Giới thiệu
ui-website = Trang web
ui-cover-image = Ảnh bìa
ui-cover = Ảnh bìa
ui-did-you-mean-one-of-these = Có phải bạn muốn nói:
ui-edit-profile = Sửa trang cá nhân
ui-theme = Giao diện
ui-font-size = Cỡ chữ
ui-post-layout = Bố cục bài viết
ui-light = Sáng
ui-dark = Tối
ui-auto = Tự động
ui-small = Nhỏ
ui-medium = Vừa
ui-large = Lớn
ui-compact = Gọn
ui-comfortable = Thoáng
ui-denser-list = Danh sách dày hơn
ui-more-spacing = Nhiều khoảng trống hơn
ui-follow = Theo dõi
ui-followers = Người theo dõi
ui-following = Đang theo dõi
ui-watch = Theo dõi
ui-member-since = Thành viên từ
ui-trust-level = Mức tin cậy
ui-warnings = Cảnh báo
ui-saved-threads = Chủ đề đã lưu
ui-no-bookmarks-yet = Bạn chưa lưu chủ đề nào
ui-watching-empty =
    Bạn chưa theo dõi chuyên mục nào. Mở một chuyên mục và chọn Theo dõi để nhận
    thông báo.
ui-muting-empty =
    Bạn chưa tắt thông báo chuyên mục nào. Mở một chuyên mục và chọn Tắt thông
    báo để ẩn nó.

## Danh mục sản phẩm & đánh giá
ui-products = Sản phẩm
ui-product-review = Đánh giá sản phẩm
ui-reviews = Đánh giá
ui-write-a-review = Viết đánh giá
ui-brand = Thương hiệu
ui-brands = Thương hiệu
ui-material = Chất liệu
ui-materials = Chất liệu
ui-origin = Xuất xứ
ui-style = Phong cách
ui-overall = Tổng thể
ui-average-rating = Điểm trung bình
ui-verified-purchase = Đã mua hàng
ui-verified = Đã xác minh
ui-no-reviews-yet = Chưa có đánh giá nào
ui-view-product = Xem sản phẩm
ui-all-categories = Tất cả chuyên mục
ui-all-categories-2 = Tất cả chuyên mục
ui-all-products = Tất cả sản phẩm
ui-highest-rated = Điểm cao nhất
ui-lowest-rated = Điểm thấp nhất
ui-top-rated = Được đánh giá cao
ui-rating-hint = Chạm vào sao để chấm điểm. Chỉ điểm tổng thể là bắt buộc.
ui-public-once-approved = Sẽ hiển thị công khai sau khi được duyệt
ui-drop-or-browse = Kéo thả tệp vào đây, hoặc bấm để chọn
ui-drop-or-choose-image = Kéo thả ảnh vào đây, hoặc bấm để chọn ảnh

## Trang cá nhân & tài khoản

ui-about = Giới thiệu
ui-actions = Thao tác
ui-activity = Hoạt động
ui-account-status = Trạng thái tài khoản
ui-summary = Tổng quan
ui-total-shown = Tổng hiển thị
ui-role = Vai trò
ui-trust-score = Điểm tin cậy
ui-security = Bảo mật
ui-preferences = Tùy chọn
ui-profile-settings = Cài đặt trang cá nhân
ui-view-public-profile = Xem trang cá nhân công khai
ui-save-profile = Lưu trang cá nhân
ui-save-preferences = Lưu tùy chọn
ui-my-bookmarks = Chủ đề đã lưu
ui-saved-threads-2 = Chủ đề đã lưu
ui-saved = Đã lưu
ui-my-reports = Báo cáo của tôi
ui-reports-you-ve-filed = Báo cáo bạn đã gửi
ui-you-haven-t-reported-anything-yet = Bạn chưa gửi báo cáo nào.
ui-save-threads-for-later-by-clicking = Lưu chủ đề để đọc sau bằng cách bấm biểu tượng dấu trang ở trang chủ đề bất kỳ.
ui-cover-image-2 = Ảnh bìa
ui-no-cover-image-a-gradient-is = Chưa có ảnh bìa — trang cá nhân sẽ hiển thị nền chuyển màu
ui-recommended-1200-400-px-jpeg-png = Khuyến nghị: 1200×400 px, JPEG/PNG/WebP, tối đa 8 MB.
ui-reach-member-trust-level-or-get-2 = Cần đạt mức tin cậy Thành viên (hoặc được xác minh) để đính kèm ảnh bìa.
ui-drop-to-set-the-cover-image = Thả để đặt làm ảnh bìa
ui-leave-blank-to-use-username = Để trống để dùng tên đăng nhập
ui-max-500-characters = Tối đa 500 ký tự.
ui-please-enter-a-valid-url-starting = Vui lòng nhập URL hợp lệ, bắt đầu bằng http:// hoặc https://
ui-manage-product = Quản lý sản phẩm
ui-manage-watched-categories = Quản lý chuyên mục đang theo dõi
ui-watching-muted = Theo dõi & Tắt thông báo
ui-watching = Đang theo dõi
ui-muted = Đã tắt thông báo
ui-to = Đến
ui-sign-in-to-watch-this-category = Đăng nhập để theo dõi chuyên mục này
ui-your-account-has-been-suspended = Tài khoản của bạn đã bị đình chỉ.
ui-see-warning-details-in-your-notifications = Xem chi tiết cảnh cáo trong mục thông báo của bạn

## Thông báo

ui-you-re-all-caught-up-no = Bạn đã xem hết — không có thông báo nào.
ui-new-follower = Người theo dõi mới
ui-started-following-you = đã bắt đầu theo dõi bạn
ui-someone-started-following-you = Có người bắt đầu theo dõi bạn
ui-you-received-a-warning = Bạn nhận được một cảnh cáo

## Đăng nhập, đăng ký & mật khẩu

ui-sign-in-to-your-account-to = Đăng nhập vào tài khoản của bạn để tiếp tục.
ui-join-in = Tham gia
ui-get-involved = Tham gia cùng cộng đồng
ui-registration-is-currently-closed-please-contact = Hiện đã đóng đăng ký. Vui lòng liên hệ quản trị viên.
ui-letters-numbers-underscores-hyphens-cannot-be = Chữ cái, số, gạch dưới, gạch ngang. Không thể thay đổi về sau.
ui-minimum-8-characters-including-a-digit = Tối thiểu 8 ký tự, gồm ít nhất một chữ số và một ký tự đặc biệt.
ui-must-include-a-digit-and-a = Phải có ít nhất một chữ số và một ký tự đặc biệt.
ui-passwords-do-not-match = Mật khẩu nhập lại không khớp.
ui-choose-a-strong-password-for-your = Hãy chọn một mật khẩu mạnh cho tài khoản của bạn.
ui-tips-for-a-strong-password = Mẹo tạo mật khẩu mạnh
ui-use-at-least-8-characters = Dùng ít nhất 8 ký tự
ui-mix-letters-numbers-symbols = Kết hợp chữ cái, số và ký tự đặc biệt
ui-don-t-reuse-passwords-from-other = Đừng dùng lại mật khẩu của các trang khác
ui-enter-your-email-and-we-ll = Nhập email của bạn, chúng tôi sẽ gửi liên kết đặt lại mật khẩu.
ui-invalid-or-missing-reset-token = Mã đặt lại mật khẩu không hợp lệ hoặc bị thiếu.
ui-request-a-new-link = Yêu cầu liên kết mới
ui-request-a-new-one = Yêu cầu liên kết mới.
ui-that-verification-link-is-invalid-or = Liên kết xác minh không hợp lệ hoặc đã hết hạn.
ui-your-email-has-been-verified-you = Email của bạn đã được xác minh. Bạn có thể đăng nhập ngay.
ui-please-verify-your-email-address-before = Vui lòng xác minh địa chỉ email của bạn trước khi tiếp tục.
ui-resend-verification-email = Gửi lại email xác minh
ui-your-account-is-temporarily-locked-due = Tài khoản của bạn tạm thời bị khóa do đăng nhập sai nhiều lần. Vui lòng thử lại sau.

## Soạn bài & trả lời

ui-new-post = Bài viết mới
ui-new-thread-2 = Tạo chủ đề
ui-choose-a-category = Chọn chuyên mục…
ui-select-a-category = Chọn chuyên mục…
ui-write-a-clear-specific-title = Viết tiêu đề rõ ràng, cụ thể
ui-write-your-post-in-markdown = Viết bài của bạn bằng Markdown…
ui-press-enter-or-comma-to-add = Nhấn Enter hoặc dấu phẩy để thêm. Tối đa 5 thẻ.
ui-press-enter-or-comma-to-add-2 = Nhấn Enter hoặc dấu phẩy để thêm. Tối đa 5 thẻ.
ui-remove-tag = Xóa thẻ
ui-posting = Đang đăng
ui-post-reply = Gửi trả lời
ui-reply-to-thread = Trả lời chủ đề
ui-post-actions = Thao tác với bài viết
ui-this-post-has-been-deleted = Bài viết này đã bị xóa.
ui-this-thread-is-locked-and-no = Chủ đề này đã bị khóa và không nhận thêm trả lời.
ui-your-account-does-not-yet-have = Tài khoản của bạn chưa có quyền trả lời trong chuyên mục này. Hãy xác minh email hoặc tham gia thêm để mở khóa quyền đăng bài.
ui-awaiting-moderator-approval-only-visible-to = Đang chờ kiểm duyệt — chỉ bạn và ban quản trị nhìn thấy
ui-pending-approval = Chờ duyệt
ui-thread-info = Thông tin chủ đề
ui-subcategories = Chuyên mục con
ui-unanswered = Chưa trả lời

## Ảnh & tải lên

ui-thumbnail-preview = Xem trước ảnh đại diện
ui-remove-thumbnail = Xóa ảnh đại diện
ui-drop-to-set-thumbnail = Thả để đặt làm ảnh đại diện
ui-close = Đóng
ui-jpeg-png-webp-gif-max-10 = JPEG · PNG · WebP · GIF · tối đa 10 MB
ui-recommended-1280-720-px-16-9 = Khuyến nghị 1280 × 720 px (16:9)
ui-1280-720-px-16-9-recommended = Khuyến nghị 1280 × 720 px (16:9)
ui-reach-member-trust-level-or-get = Cần đạt mức tin cậy Thành viên (hoặc được xác minh) để đính kèm ảnh đại diện chủ đề.
ui-uploading = Đang tải lên
ui-uploading-2 = Đang tải lên…
ui-change = Đổi
ui-remove = Gỡ bỏ

## Kiểm duyệt & báo cáo

ui-moderator-actions = Thao tác kiểm duyệt
ui-move = Chuyển
ui-move-thread-2 = Chuyển chủ đề
ui-moving-a-thread-to-a-different = Chuyển chủ đề sang chuyên mục khác là thao tác của kiểm duyệt viên.
ui-select-the-target-category-for-this = Chọn chuyên mục đích cho chủ đề này.
ui-report = Báo cáo
ui-reporting-post-by = Báo cáo bài viết của
ui-describe-why-this-post-violates-the = Mô tả vì sao bài viết này vi phạm nội quy…
ui-submit-report = Gửi báo cáo

## Tìm kiếm

ui-search-all-categories = Tìm trong tất cả chuyên mục
ui-no-results-found-for = Không tìm thấy kết quả nào cho "
ui-search-tips = Mẹo tìm kiếm
ui-use-specific-keywords = Dùng từ khóa cụ thể
ui-tip-searches-products-and-threads = Tìm trong tên sản phẩm, thương hiệu, tiêu đề chủ đề và nội dung bài trả lời
ui-tip-accents-optional = Không cần gõ dấu — “ghe an” vẫn ra “ghế ăn”
ui-tip-filter-products-by-brand = Ở tab Sản phẩm, lọc thêm theo loại, thương hiệu hoặc chất liệu
ui-sort-relevance = Liên quan nhất
ui-sort-most-replies = Nhiều trả lời nhất
# Bộ lọc chuyên mục áp cho cả hai loại; các facet sản phẩm thì không. Ghi rõ
# ngay cạnh control là cách duy nhất để người đọc không tưởng bộ lọc thương
# hiệu cũng làm giảm số thảo luận.
# Cây phân loại của chính danh mục sản phẩm (Sofa, Ghế, Bàn…), không phải của
# diễn đàn. Đặt tên theo thứ nó phân loại để không bị nhầm với chuyên mục thảo luận.
ui-product-category = Danh mục
ui-too-many-filters = Không có kết quả nào khớp tổ hợp bộ lọc này.
ui-drop-product-filters =
    { $count ->
       *[other] Bỏ bộ lọc sản phẩm: { $count } sản phẩm
    }
ui-drop-category-filter =
    { $count ->
       *[other] Bỏ lọc chuyên mục: { $count } kết quả
    }
ui-search-result-types = Loại kết quả
ui-search-tab-all = Tất cả
ui-search-tab-products = Sản phẩm
ui-search-tab-discussions = Thảo luận
ui-see-all-n-products =
    { $count ->
       *[other] Xem cả { $count } sản phẩm
    }
ui-see-n-products-instead =
    { $count ->
       *[other] Xem { $count } sản phẩm khớp
    }
ui-see-n-discussions-instead =
    { $count ->
       *[other] Xem { $count } thảo luận khớp
    }
ui-shorter-queries-return-more-results = Truy vấn ngắn hơn cho nhiều kết quả hơn
ui-use-the-category-filter-to-narrow = Dùng bộ lọc chuyên mục để thu hẹp kết quả
ui-search-is-temporarily-unavailable-please-try = Chức năng tìm kiếm tạm thời không khả dụng. Vui lòng thử lại sau giây lát.
ui-no-posts-found-for-the-tag = Không tìm thấy bài viết nào với thẻ

## Trạng thái trống & lời mời tham gia

ui-be-the-first = Hãy là người đầu tiên.
ui-no-discussions-yet-be-the-first = Chưa có thảo luận nào. Hãy là người mở đầu!
ui-start-the-first-thread = Tạo chủ đề đầu tiên
ui-no-categories-yet-an-admin-needs = Chưa có chuyên mục nào. Quản trị viên cần tạo trước.
ui-no-posts-have-been-marked-solved = Chưa có bài viết nào được đánh dấu đã giải quyết.
ui-nothing-left-unanswered-nice-work = Không còn chủ đề nào chưa được trả lời — tuyệt vời!
ui-view-all-discussions = Xem tất cả thảo luận

## Sản phẩm & đánh giá

ui-suggest-a-product = Đề xuất sản phẩm
ui-search-products = Tìm sản phẩm…
ui-product-name = Tên sản phẩm
ui-product-images = Ảnh sản phẩm
ui-no-product-images-yet = Chưa có ảnh sản phẩm
ui-about-this-product = Về sản phẩm này
ui-type = Loại
ui-type-a-material-name-then-pick = Nhập tên chất liệu rồi chọn để thêm.
ui-type-to-search-materials = Nhập để tìm chất liệu…
ui-furniture = Nội thất
ui-room = Phòng
ui-none = — Không —
ui-optional = (không bắt buộc)
ui-photos = Hình ảnh
ui-select-if-known = (chọn nếu biết)
ui-price-from = Giá từ (₫)
ui-price-to = Giá đến (₫)
ui-price-on-request = Giá liên hệ
ui-price-range = Khoảng giá
ui-filter-by-category = Lọc theo chuyên mục:
ui-no-brands-yet = Chưa có thương hiệu nào.
ui-no-materials-yet = Chưa có chất liệu nào.
ui-pick-a-brand-to-see-all = Chọn một thương hiệu để xem toàn bộ sản phẩm của họ.
ui-pick-a-material-to-see-every = Chọn một chất liệu để xem mọi sản phẩm dùng chất liệu đó.
ui-top-rated-products = Sản phẩm được đánh giá cao
# Tiêu đề thay thế khi chưa đủ sản phẩm có lượng đánh giá để gọi là "đánh giá
# cao". Độ mới là điều catalog luôn khẳng định được, nên kệ vẫn hiện sản phẩm.
ui-newest-products = Sản phẩm mới nhất
ui-most-reviewed = Nhiều đánh giá nhất
ui-review = Đánh giá
ui-reviewing-a-product = Bạn đang đánh giá sản phẩm?
ui-optional-turns-this-post-into-a = (không bắt buộc — biến bài viết này thành bài đánh giá có chấm điểm)
ui-search-for-a-product-to-review = Tìm sản phẩm để đánh giá… (để trống nếu là bài viết thường)
ui-can-t-find-it-add-a = Không tìm thấy? Thêm sản phẩm mới
ui-add-a-product = Thêm sản phẩm
ui-add-select-for-review = Thêm & chọn để đánh giá
ui-the-product-becomes-public-once-an = Sản phẩm sẽ hiển thị công khai sau khi quản trị viên duyệt. Bài đánh giá của bạn được lưu ngay trong mọi trường hợp.
ui-your-rating = Điểm của bạn
ui-your-submission-pending = Sản phẩm này đang chờ quản trị viên duyệt — bạn vẫn thêm hoặc xoá được ảnh trong lúc chờ.
ui-your-submission-published = Bạn đã thêm sản phẩm này, giờ thuộc danh mục chung — hãy báo quản trị viên nếu cần sửa.
ui-not-rated = Chưa chấm điểm
ui-i-have-actually-bought-or-used = Tôi đã thực sự mua hoặc dùng sản phẩm này
ui-verified-purchases-only = Chỉ người đã mua hàng
ui-review-summary = Tổng quan đánh giá
ui-rating-distribution = Phân bố điểm đánh giá
ui-rating-headline-aria =
    { $count ->
        [0] { $score } trên 5 sao, chưa có đánh giá
       *[other] { $score } trên 5 sao, { $count } đánh giá
    }
ui-rating-histogram-row-aria =
    { $count ->
        [0] { $star } sao: chưa có đánh giá
       *[other] { $star } sao: { $count } đánh giá
    }
ui-scores-by-dimension = Điểm theo từng tiêu chí
ui-strongest = Điểm mạnh nhất:
ui-weakest = Điểm yếu nhất:
ui-sort-reviews = Sắp xếp đánh giá
ui-no-reviews-match-this-filter = Không có đánh giá nào khớp bộ lọc này.
ui-no-reviews-yet-be-the-first = Chưa có đánh giá nào — hãy là người đầu tiên.
ui-log-in-to-review = Đăng nhập
ui-view-your-review = Xem đánh giá của bạn
ui-you-have-already-reviewed-this-product = Bạn đã đánh giá sản phẩm này rồi. Hãy sửa bài đánh giá hiện có để cập nhật.

## Khác

ui-ferum-board = Ferum Board
ui-active = Đang hoạt động
ui-previous-2 = Trước
ui-next-2 = Sau

## Tiêu đề trang & mô tả

ui-title-forgot-password = Quên mật khẩu
ui-desc-catalog = Khám phá nội thất, chất liệu và không gian trên { $site }.
ui-desc-brands = Danh bạ các thương hiệu nội thất trên { $site }.
ui-desc-materials = Tra cứu chất liệu nội thất và duyệt sản phẩm theo chất liệu trên { $site }.
ui-desc-thread = { $title } — { $author }
ui-breadcrumb = Đường dẫn phân cấp

## Danh từ đếm được
# Tiếng Việt không biến đổi theo số lượng nên chỉ có một dạng.

ui-products-label =
    { $count ->
       *[other] sản phẩm
    }
ui-threads-label =
    { $count ->
       *[other] chủ đề
    }
ui-reviews-label =
    { $count ->
       *[other] đánh giá
    }
ui-results-label =
    { $count ->
       *[other] kết quả cho
    }
ui-page-x-of-y = trang { $page }/{ $total }

## Trạng thái tài khoản

ui-account-suspended = Tài khoản của bạn đã bị đình chỉ.
ui-account-suspended-until = Tài khoản của bạn bị đình chỉ đến { $date }.
ui-change-cover = Đổi ảnh bìa
ui-upload-cover = Tải ảnh bìa lên
ui-change-photo = Đổi ảnh đại diện
ui-upload-photo = Tải ảnh đại diện lên

## Nội dung thông báo

ui-notification = Thông báo
ui-issued-a-warning = đã gửi một cảnh cáo
ui-a-moderator-issued-a-warning = Một kiểm duyệt viên đã gửi cảnh cáo
ui-new-reply-on-your-thread = Có trả lời mới trong chủ đề của bạn
ui-you-were-mentioned = Bạn được nhắc đến
ui-someone-reacted-to-your-post = Có người bày tỏ cảm xúc với bài viết của bạn
ui-best-answer-marked = Đã chọn câu trả lời hay nhất
ui-replied-to-your-thread = đã trả lời chủ đề của bạn
ui-someone-replied-to-your-thread = Có người đã trả lời chủ đề của bạn
ui-mentioned-you-in-a-post = đã nhắc đến bạn trong một bài viết
ui-you-were-mentioned-in-a-post = Bạn được nhắc đến trong một bài viết
ui-reacted = đã bày tỏ cảm xúc
ui-someone-reacted = Có người đã bày tỏ cảm xúc
ui-to-your-post = với bài viết của bạn
ui-marked-your-post-as-best-answer = đã chọn bài viết của bạn là câu trả lời hay nhất
ui-your-post-was-marked-as-best-answer = Bài viết của bạn đã được chọn là câu trả lời hay nhất

## Soạn bài & thao tác kiểm duyệt

ui-publish = Đăng bài
ui-pin = Ghim
ui-unpin = Bỏ ghim
ui-lock = Khóa
ui-unlock = Mở khóa

## Tiêu chí đánh giá

ui-durability = Độ bền
ui-comfort = Độ thoải mái
ui-aesthetics = Thẩm mỹ
ui-value-for-money = Đáng đồng tiền
ui-value = Giá trị
ui-out-of-5 = trên 5

## Chính sách chuyên mục

ui-policy-members = Thành viên
ui-policy-trusted = Thành viên tin cậy
ui-policy-staff-only = Chỉ ban quản trị
ui-policy-closed = Đã đóng

## Loại sản phẩm & nhóm chất liệu

ui-product-type-furniture = Nội thất
ui-product-type-material = Chất liệu
ui-product-type-room = Không gian
ui-material-wood-natural = Gỗ tự nhiên
ui-material-wood-engineered = Gỗ công nghiệp
ui-material-rattan-bamboo = Mây & tre
ui-material-metal = Kim loại
ui-material-fabric = Vải
ui-material-leather = Da
ui-material-stone = Đá
ui-material-glass = Kính
ui-material-plastic = Nhựa
ui-material-other = Khác

## Trạng thái trống & lời mời

ui-join-today = Tham gia { $site } ngay hôm nay.
ui-join-the-community-and-start = Tham gia cộng đồng và bắt đầu thảo luận.
ui-join-the-conversation-and-share = Tham gia thảo luận và chia sẻ suy nghĩ của bạn.
ui-personalised-from-categories = Cá nhân hóa từ { $count } chuyên mục
ui-no-unanswered-threads = Không còn chủ đề nào chưa được trả lời — tuyệt vời!
ui-no-solved-threads-yet = Chuyên mục này chưa có chủ đề nào đã giải quyết.
ui-no-threads-in-this-category = Chuyên mục này chưa có chủ đề nào.
ui-no-products-found-for-query = Không tìm thấy sản phẩm nào cho “{ $query }”.
ui-no-products-match-this-filter = Không có sản phẩm nào khớp bộ lọc này.
ui-suggest-a-product-named = Đề xuất sản phẩm “{ $query }”
ui-no-reviews-yet-period = Chưa có đánh giá nào.
ui-in-parent-category = Trong { $category }
ui-related = Liên quan
ui-in-this-category = trong chuyên mục này

## Giao diện đánh giá
# Nội dung riêng của theme ferum-review (phần masthead trên trang chủ) — theme
# default không có chuỗi tương ứng. Xem
# frontend/themes/ferum-review/templates/home.html.

ui-material-furniture-reviews = Đánh giá vật liệu & nội thất
ui-real-reviews-from-the-people-who-built-it = Đánh giá thực tế từ những người đã thi công và sử dụng
ui-architects-contractors-log-the-numbers =
    Kiến trúc sư, nhà thầu và chủ nhà ghi lại những con số quan trọng: độ bền
    theo thời gian, khả năng chống ẩm, độ khó khi thi công và chi phí thực tế
    trên mỗi m².
# Đồng thời là nhãn nút phụ ở masthead (→ /catalog). Dùng lại thay vì đặt thêm
# key kiểu "duyệt danh mục": đích đến chỉ có một tên, và nên đọc giống nhau ở mọi
# chỗ trỏ tới nó.
ui-product-catalog = Danh mục sản phẩm
ui-discussion-categories = Chuyên mục thảo luận
ui-browse-by-type = Duyệt theo loại
ui-featured-image = Ảnh nổi bật

# Hộp thoại xác nhận + thanh đo độ mạnh mật khẩu
js-confirm = Xác nhận
js-type-to-confirm = Nhập { $code } để xác nhận:
js-pw-very-weak = Rất yếu
js-pw-weak = Yếu
js-pw-fair = Trung bình
js-pw-strong = Mạnh
js-pw-very-strong = Rất mạnh

# Trang tài khoản
js-invalid-website-url = Vui lòng nhập URL hợp lệ, bắt đầu bằng http:// hoặc https://
js-profile-saved = Đã lưu trang cá nhân.
js-failed-to-save = Lưu không thành công.
js-avatar-invalid-type = Ảnh đại diện phải là ảnh JPEG, PNG, GIF hoặc WebP.
js-avatar-too-large = Ảnh đại diện phải nhỏ hơn 5 MB.
js-avatar-updated = Đã cập nhật ảnh đại diện.
js-avatar-removed = Đã xóa ảnh đại diện.
js-failed-remove-avatar = Không xóa được ảnh đại diện.
js-cover-invalid-type = Ảnh bìa phải là ảnh JPEG, PNG, GIF hoặc WebP.
js-cover-too-large = Ảnh bìa phải nhỏ hơn 8 MB.
js-cover-updated = Đã cập nhật ảnh bìa.
js-cover-removed = Đã xóa ảnh bìa.
js-failed-remove-cover = Không xóa được ảnh bìa.
js-upload-failed = Tải lên không thành công.
js-password-changed = Đã đổi mật khẩu thành công.
js-failed-change-password = Không đổi được mật khẩu.
js-preferences-saved = Đã lưu tùy chọn.
js-failed-save-preferences = Không lưu được tùy chọn.

# Trang đăng nhập / đăng ký
js-invalid-credentials = Email hoặc mật khẩu không đúng.
js-registration-failed = Đăng ký không thành công. Vui lòng thử lại.
js-resend-in = Gửi lại sau { $seconds } giây
js-reset-failed = Đặt lại mật khẩu không thành công. Liên kết có thể đã hết hạn.

# Soạn bài
js-thumbnail-invalid-type = Ảnh đại diện chủ đề phải là ảnh JPEG, PNG, GIF hoặc WebP.
js-thumbnail-too-large = Ảnh đại diện chủ đề phải nhỏ hơn 10 MB.
js-failed-save-changes = Không lưu được thay đổi.
js-could-not-load-materials = Không tải được danh sách chất liệu.
js-no-materials-in-catalog = Danh mục chưa có chất liệu nào.
js-could-not-add-product = Không thêm được sản phẩm.
js-delete-product-confirm = Xóa
js-material-links = Liên kết chất liệu
js-photos = Hình ảnh
js-reviews = Đánh giá
js-product-delete-blocked =
    Sản phẩm này đang có { $count } bài đánh giá. Xóa đi sẽ khiến các bài đó không
    còn sản phẩm để đánh giá, nên việc xóa vĩnh viễn bị chặn. Hãy lưu trữ thay thế —
    sản phẩm biến mất khỏi danh mục và mọi bài đánh giá vẫn còn nguyên.
js-product-delete-permanent-warning =
    Không có gì tham chiếu tới sản phẩm này nên có thể xóa hẳn. Ảnh của nó cũng được
    giải phóng khỏi kho lưu trữ. Thao tác này không thể hoàn tác.
js-product-added-photos-failed =
    Đã thêm sản phẩm, nhưng một số hình ảnh không tải lên được. Bạn có thể thêm
    lại sau khi sản phẩm được duyệt.
js-remove-photo = Xoá hình
js-remove-material = Bỏ chất liệu
js-no-materials-found = Không có chất liệu nào khớp.
js-price-from-exceeds-to = "Giá từ" không được lớn hơn "giá đến".
js-use-this-product = Chọn sản phẩm này
js-give-overall-score = Vui lòng chấm ít nhất điểm Tổng thể cho sản phẩm.
js-could-not-publish = Không đăng được. Vui lòng thử lại.
js-write-a-review = Viết đánh giá
js-new-post = Bài viết mới
js-publish = Đăng bài
js-publish-review = Đăng đánh giá

# Nhãn loại sản phẩm & nhóm chất liệu
js-product-type-furniture = Nội thất
js-product-type-material = Chất liệu
js-product-type-room = Không gian
js-material-wood-natural = Gỗ tự nhiên
js-material-wood-engineered = Gỗ công nghiệp
js-material-rattan-bamboo = Mây & tre
js-material-metal = Kim loại
js-material-fabric = Vải
js-material-leather = Da
js-material-stone = Đá
js-material-glass = Kính
js-material-plastic = Nhựa
js-material-other = Khác

# Trang cá nhân
js-nobody-here-yet = Chưa có ai ở đây.
js-no-posts-yet = Chưa có bài viết nào.
js-view-thread = Xem chủ đề

# Trang chủ đề
js-failed-delete-post = Không xóa được bài viết.
js-failed-submit-report = Không gửi được báo cáo.

# Trình soạn thảo
js-composer-write = Soạn
js-composer-preview = Xem trước
js-composer-placeholder = Viết trả lời của bạn bằng Markdown…
js-composer-nothing-to-preview = Chưa có gì để xem trước.
js-composer-hint = Hỗ trợ Markdown · Ctrl+B / Ctrl+I / Ctrl+K
js-composer-toolbar = Thanh công cụ định dạng
js-composer-bold = In đậm (Ctrl+B)
js-composer-italic = In nghiêng (Ctrl+I)
js-composer-strikethrough = Gạch ngang
js-composer-heading-1 = Tiêu đề 1
js-composer-heading-2 = Tiêu đề 2
js-composer-quote = Trích dẫn
js-composer-code-inline = Mã (trong dòng)
js-composer-code-block = Khối mã
js-composer-link = Liên kết (Ctrl+K)
js-composer-ul = Danh sách không thứ tự
js-composer-ol = Danh sách có thứ tự
js-composer-attach-image = Đính kèm ảnh
js-failed-upload-image = Tải ảnh lên không thành công.

# Tiện ích khác
js-bookmark = Lưu
js-bookmarked = Đã lưu
js-bookmark-this-thread = Lưu chủ đề này
js-remove-bookmark = Bỏ lưu
js-clear = Xóa
js-clear-selection = Bỏ chọn
js-type-to-search = Nhập để tìm…
js-search = Tìm kiếm
js-no-matches = Không có kết quả khớp

# Kệ sản phẩm trang chủ. Số lượng là số card bị breakpoint hiện tại ẩn đi, nên
# được đo ở trình duyệt chứ không render từ server.
js-show-more-products = Xem thêm { $count } sản phẩm

# Xác nhận & lỗi ở trang chủ đề
js-delete-post-title = Xóa bài viết
js-delete-post-body = Xóa bài viết này? Thao tác không thể hoàn tác.
js-delete = Xóa
js-failed-post-reply = Không gửi được trả lời.
js-mark-best-answer-title = Chọn câu trả lời hay nhất
js-mark-best-answer-body = Đánh dấu bài viết này là câu trả lời hay nhất?
js-mark-as-best = Chọn làm hay nhất
js-failed-mark-best-answer = Không đánh dấu được câu trả lời hay nhất.
js-failed-move-thread = Không chuyển được chủ đề.
js-delete-thread-title = Xóa chủ đề
js-delete-thread-body = Xóa toàn bộ chủ đề này? Thao tác không thể hoàn tác.
js-delete-thread-ok = Xóa chủ đề
js-failed-delete-thread = Không xóa được chủ đề.
js-failed-thread-action = Không cập nhật được chủ đề. Vui lòng thử lại.

# Chuỗi riêng của tiện ích
js-notifications = Thông báo
js-unread-count = { $count } chưa đọc
js-loading = Đang tải…
js-scroll-for-more = Cuộn để xem thêm…
js-cannot-react-own-post = Bạn không thể bày tỏ cảm xúc với bài viết của chính mình.
js-react-trust-insufficient = Tài khoản của bạn cần được xác minh để bày tỏ cảm xúc.
js-account-suspended = Tài khoản của bạn đã bị đình chỉ.

## ─── Bộ lọc tìm kiếm ─────────────────────────────────────────────────────────
# Tùy chọn mặc định của mỗi ô lọc là TÊN của chính nó, không phải "Tất cả …".
# Bốn dropdown cùng mở đầu bằng một chữ thì mắt không quét được — phải đọc hết
# từng cái mới phân biệt nổi.
ui-filter-by = Lọc theo:
ui-active-filters = Bộ lọc đang áp dụng
ui-clear-all-filters = Bỏ tất cả
