# Thông báo lỗi tiếng Việt.
#
# Khóa nào thiếu ở đây sẽ tự động dùng bản tiếng Anh, nên bản dịch một phần vẫn
# chạy được. Ưu tiên dịch các lỗi người dùng thường gặp nhất.

## ─── Chung ───────────────────────────────────────────────────────────────────

error-unauthorized = Bạn cần đăng nhập để làm việc này.
error-not-found = Không tìm thấy nội dung bạn cần.
error-internal-error = Đã có lỗi xảy ra ở phía chúng tôi. Vui lòng thử lại.
error-validation-error = Một số thông tin bạn nhập chưa hợp lệ.
error-rate-limit-exceeded =
    Bạn thao tác quá nhanh. Vui lòng thử lại sau { $seconds } giây.
error-page-out-of-range = Trang này nằm quá xa. Bạn có thể xem đến trang { $max_page } — hãy dùng tìm kiếm nếu cần tìm một nội dung cụ thể.

## ─── Quyền hạn ───────────────────────────────────────────────────────────────

error-permission-denied = Bạn không có quyền làm việc này.
error-trust-level-insufficient =
    Tài khoản của bạn cần mức độ tin cậy cao hơn. Hãy tiếp tục tham gia để nâng cấp.
error-not-author = Bạn chỉ có thể thao tác với nội dung của chính mình.

## ─── Tài khoản ───────────────────────────────────────────────────────────────

error-account-suspended = Tài khoản của bạn đã bị đình chỉ.
error-account-locked =
    Tài khoản tạm khóa do đăng nhập sai nhiều lần. Vui lòng thử lại sau.
error-email-not-verified = Vui lòng xác minh email trước khi tiếp tục.
error-incorrect-current-password = Mật khẩu hiện tại không đúng.
error-registration-closed = Hiện chưa mở đăng ký.
error-registration-conflict = Email hoặc tên đăng nhập này đã được sử dụng.
error-email-taken = Email này đã được đăng ký.
error-username-taken = Tên đăng nhập này đã có người dùng.
error-invalid-or-expired-token = Liên kết không hợp lệ hoặc đã hết hạn.
error-token-already-used = Liên kết này đã được sử dụng.
error-password-requirements =
    Mật khẩu phải có ít nhất 8 ký tự, gồm ít nhất một chữ số và một ký tự đặc biệt.
error-invalid-username-format =
    Tên đăng nhập phải dài 3–30 ký tự, chỉ gồm chữ, số, gạch dưới hoặc gạch ngang.

## ─── Nội dung ────────────────────────────────────────────────────────────────

error-thread-locked = Chủ đề này đã bị khóa.
error-category-closed = Chuyên mục này không nhận bài viết mới.
error-edit-window-expired = Đã hết thời hạn chỉnh sửa nội dung này.
error-post-content-empty = Bài viết không được để trống.
error-post-content-too-long =
    Bài viết quá dài. Giới hạn là { $limit_kb } KB.
error-cannot-react-to-own-post = Bạn không thể bày tỏ cảm xúc với bài của chính mình.
error-reaction-exists = Bạn đã bày tỏ cảm xúc này rồi.
error-slug-taken = Tên này đã được sử dụng.

## ─── Tải lên ─────────────────────────────────────────────────────────────────

error-upload-quota-exceeded = Bạn đã đạt giới hạn tải lên trong ngày. Hãy thử lại vào ngày mai.
error-avatar-too-large = Ảnh đại diện phải nhỏ hơn hoặc bằng { $limit_mb } MB.
error-image-invalid-type = Ảnh phải ở định dạng JPEG, PNG, WebP hoặc GIF.

## ─── Ngôn ngữ ────────────────────────────────────────────────────────────────

error-locale-not-enabled = Ngôn ngữ này không khả dụng trên trang.

## ─── Quyền & vai trò ─────────────────────────────────────────────────────────

error-tag-create-permission-required = Bạn không có quyền tạo thẻ mới.
error-cannot-grant-permissions-you-lack =
    Bạn không thể cấp quyền mà chính bạn cũng không có.
error-cannot-modify-system-role-permissions = Không thể sửa quyền của vai trò hệ thống.
error-cannot-delete-system-role = Không thể xóa vai trò hệ thống.
error-cannot-remove-last-admin = Bạn không thể gỡ bỏ quản trị viên cuối cùng.
error-role-already-assigned = Vai trò này đã được gán cho người dùng.
error-no-password-set = Tài khoản này chưa đặt mật khẩu.
error-ban-expiry-must-be-future = Ngày hết hạn cấm phải ở tương lai.
error-invalid-trust-level = Chọn một trong: new, basic, member, regular hoặc leader.

## ─── Chuyên mục & chủ đề ─────────────────────────────────────────────────────

error-slug-reserved = Tên này đã được dành riêng và không thể sử dụng.
error-category-nesting-too-deep =
    Chuyên mục chỉ có thể lồng nhau tối đa { $max_depth } cấp.
error-category-cannot-be-its-own-parent = Một chuyên mục không thể là cha của chính nó.
error-category-has-subcategories = Chuyên mục này vẫn còn chuyên mục con nên không thể xóa.
error-category-has-threads = Chuyên mục này vẫn còn chủ đề nên không thể xóa.
error-invalid-view-policy = Đây không phải là thiết lập hiển thị hợp lệ.
error-invalid-post-policy = Đây không phải là thiết lập đăng bài hợp lệ.
error-thread-title-length = Tiêu đề chủ đề phải dài từ 5 đến 255 ký tự.
error-parent-post-wrong-thread = Bài trả lời đó không thuộc chủ đề này.
error-best-answer-wrong-thread = Câu trả lời hay nhất phải là một bài viết trong chủ đề này.
error-post-not-pending-approval = Bài viết này không đang chờ duyệt.
error-invalid-reaction-kind = Đây không phải là cảm xúc bạn có thể dùng.
error-invalid-status = Đây không phải là trạng thái hợp lệ.
error-invalid-smtp-port = Cổng SMTP phải là số nguyên từ 1 đến 65535.

## ─── Báo cáo ─────────────────────────────────────────────────────────────────

error-report-target-required = Báo cáo phải tham chiếu tới một bài viết hoặc một chủ đề.
error-report-already-resolved = Báo cáo này đã được xử lý.
error-reason-too-long =
    Vui lòng viết lý do trong vòng { $limit } ký tự.
error-ban-duration-too-long =
    Khoá tạm thời tối đa { $max_days } ngày. Lâu hơn thế là khoá vĩnh viễn và
    cần quyền khoá vĩnh viễn.
error-ban-until-in-past =
    Thời điểm hết hạn khoá phải nằm ở tương lai. Một mốc đã qua sẽ ghi nhận lệnh
    khoá nhưng lệnh đó không có hiệu lực.
error-cannot-moderate-self =
    Bạn không thể tự áp dụng hành động kiểm duyệt lên tài khoản của chính mình.
error-cannot-moderate-staff =
    Chỉ quản trị viên mới có thể cảnh cáo hoặc khoá một điều hành viên hay một
    quản trị viên khác.

## ─── Sản phẩm & đánh giá ─────────────────────────────────────────────────────

error-product-already-reviewed =
    Bạn đã đánh giá sản phẩm này rồi. Hãy sửa bài đánh giá hiện có.
error-product-name-required = Vui lòng nhập tên sản phẩm.
error-brand-name-required = Vui lòng nhập tên thương hiệu.
error-material-name-required = Vui lòng nhập tên chất liệu.
error-invalid-product-type = Đây không phải là loại sản phẩm hợp lệ.
error-price-negative = Giá không thể là số âm.
error-price-range-inverted = Giá tối đa không được thấp hơn giá tối thiểu.
error-rating-out-of-range = Điểm đánh giá phải nằm trong khoảng { $min } đến { $max }.
error-thread-not-review = Chủ đề này không phải là một bài đánh giá sản phẩm.
error-product-has-reviews =
    Không thể xóa sản phẩm này khi vẫn còn đánh giá trỏ tới nó. Hãy lưu trữ nó
    thay vì xóa — cách đó ẩn sản phẩm khỏi danh mục và giữ nguyên các đánh giá.
error-product-media-limit =
    Sản phẩm này đã đạt giới hạn { $limit } ảnh. Hãy gỡ bớt một ảnh trước khi
    thêm ảnh mới.

## ─── Tải tệp lên ─────────────────────────────────────────────────────────────

error-file-field-missing = Không có tệp nào trong yêu cầu tải lên.
error-from-email-required = Cần có địa chỉ người gửi trước khi có thể gửi email.
error-from-email-invalid = Địa chỉ người gửi đó không phải là email hợp lệ.
error-mail-not-configured = Chức năng gửi email chưa được cấu hình, nên không gửi được thư này.
error-image-field-missing = Không có ảnh nào trong yêu cầu tải lên.
error-image-too-large = Ảnh này quá lớn. Giới hạn là { $limit_mb } MB.
error-image-content-mismatch = Tệp này không thuộc định dạng ảnh được hỗ trợ.
error-avatar-invalid-type = Ảnh đại diện phải là ảnh JPEG, PNG, WebP hoặc GIF.
error-cover-invalid-type = Ảnh bìa phải là ảnh JPEG, PNG, WebP hoặc GIF.
error-cover-too-large = Ảnh bìa phải nhỏ hơn hoặc bằng { $limit_mb } MB.
error-attachment-invalid-type = Tệp đính kèm phải là ảnh JPEG, PNG, WebP hoặc GIF.
error-attachment-too-large = Tệp đính kèm phải nhỏ hơn hoặc bằng { $limit_mb } MB.
error-thumbnail-invalid-type = Ảnh đại diện chủ đề phải là ảnh JPEG, PNG, WebP hoặc GIF.
error-thumbnail-too-large = Ảnh đại diện chủ đề phải nhỏ hơn hoặc bằng { $limit_mb } MB.
error-media-invalid-type = Ảnh phải ở định dạng JPEG, PNG, WebP hoặc GIF.
error-logo-too-large = Logo phải nhỏ hơn hoặc bằng { $limit_mb } MB.
error-logo-invalid-type = Logo phải là ảnh JPEG, PNG, WebP hoặc GIF.
error-favicon-too-large = Favicon phải nhỏ hơn hoặc bằng { $limit_kb } KB.
error-favicon-invalid-type = Favicon phải là ảnh ICO, PNG, GIF hoặc JPEG. Không chấp nhận SVG.

## ─── Hồ sơ & tùy chọn ────────────────────────────────────────────────────────

error-display-name-length = Tên hiển thị phải dài từ 1 đến 60 ký tự.
error-bio-too-long = Tiểu sử phải có tối đa 500 ký tự.
error-invalid-website-url =
    Nhập địa chỉ http:// hoặc https:// hợp lệ, tối đa 255 ký tự.
error-invalid-theme = Chọn một trong: auto, light hoặc dark.
error-invalid-font-size = Chọn một trong: small, medium hoặc large.
error-invalid-layout = Chọn compact hoặc comfortable.

## ─── Webhook ─────────────────────────────────────────────────────────────────

error-webhook-url-required = Vui lòng nhập URL webhook.
error-webhook-url-invalid = URL webhook này không hợp lệ.
error-webhook-url-missing-host = URL webhook cần có tên máy chủ.
error-webhook-url-scheme = URL webhook phải bắt đầu bằng http:// hoặc https://.
error-webhook-url-private-address =
    URL webhook không được trỏ tới địa chỉ nội bộ, loopback hoặc link-local.
error-webhook-events-required = Hãy chọn ít nhất một sự kiện để gửi.

## ─── Plugin ──────────────────────────────────────────────────────────────────

error-plugin-already-installed = Plugin này đã được cài đặt.
error-media-capability-not-granted = Plugin này không được phép tải ảnh lên.
error-rpc-action-not-granted = Thao tác này của plugin không khả dụng.
error-sql-not-allowed = Thao tác cơ sở dữ liệu này không được phép.
error-multiple-sql-statements = Mỗi lần chỉ được chạy một câu lệnh cơ sở dữ liệu.
error-sql-quoted-identifiers-not-allowed = Truy vấn của plugin không được dùng định danh trong dấu nháy kép. Hãy gọi thẳng tên bảng của plugin, không kèm dấu nháy.
error-payload-too-large = Yêu cầu này quá lớn. Nếu bạn đang tải tệp lên, hãy dùng nút tải lên thay vì dán trực tiếp nội dung tệp.
error-archive-unsafe-path = Gói này chứa đường dẫn tệp không an toàn và đã bị từ chối.
error-archive-path-traversal = Gói này chứa mục vượt cấp thư mục và đã bị từ chối.
error-archive-entry-escapes-dir =
    Gói này cố ghi ra ngoài thư mục của chính nó và đã bị từ chối.
error-archive-missing-manifest = Gói này không có plugin.toml ở thư mục gốc.
error-package-too-large = Gói này quá lớn. Giới hạn là { $limit_mb } MB.
error-seed-data-unavailable = Bản build này không kèm bộ dữ liệu mẫu nên không thể tạo. Hãy bỏ chọn ô đó rồi hoàn tất cài đặt — diễn đàn vẫn hoạt động y hệt, chỉ là không có nội dung demo.
error-invalid-granted-capabilities = Thiết lập quyền của plugin không hợp lệ.
error-plugin-manifest-missing-meta = plugin.toml thiếu mục [meta].
error-plugin-manifest-missing-id = plugin.toml thiếu meta.id.
error-plugin-manifest-missing-name = plugin.toml thiếu meta.name.
error-plugin-manifest-missing-tier = plugin.toml thiếu meta.tier.
error-plugin-manifest-missing-version = plugin.toml thiếu meta.version.
error-plugin-manifest-missing-bundle-file =
    Manifest của plugin dạng script thiếu script.bundle_file.
error-plugin-id-length = ID plugin phải dài từ 1 đến 256 ký tự.
error-plugin-id-charset =
    ID plugin chỉ được chứa chữ cái, số, dấu chấm, gạch ngang và gạch dưới.
error-plugin-id-dot-boundary = ID plugin không được bắt đầu hoặc kết thúc bằng dấu chấm.
error-confirmation-slug-mismatch = Tên bạn nhập không khớp với tên của plugin.
error-slot-name-required = Vui lòng nhập tên vị trí (slot).

## ─── Ngôn ngữ ────────────────────────────────────────────────────────────────

error-cannot-disable-last-locale =
    Bạn không thể tắt ngôn ngữ duy nhất của trang. Hãy thêm một ngôn ngữ khác trước.
error-cannot-disable-default-locale =
    Bạn không thể tắt ngôn ngữ mặc định của trang. Hãy đặt một ngôn ngữ khác làm
    mặc định trước.

## ─── Cấu hình trang ──────────────────────────────────────────────────────────

error-invalid-timezone =
    "{ $tz }" không phải là múi giờ mà máy chủ nhận ra. Hãy dùng tên IANA, ví dụ
    UTC hoặc Asia/Ho_Chi_Minh.
