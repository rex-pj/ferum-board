# Nội dung email giao dịch — tiếng Việt.
#
# Email được viết bằng ngôn ngữ của *người nhận*, xác định khi công việc được
# đưa vào hàng đợi (xem `ForumJob`), không phải ngôn ngữ của người kích hoạt.
#
# `{ $url }` được chèn dưới dạng văn bản thuần vào cả href lẫn nhãn liên kết.
# Giữ nó trên một dòng riêng để người dịch không vô tình bọc dấu câu quanh nó,
# vì dấu câu đó sẽ lọt vào bên trong URL.

## ─── Xác minh email ──────────────────────────────────────────────────────────

email-verify-subject = Xác minh địa chỉ email của bạn
email-verify-body =
    <p>Chào mừng bạn đến với { $site_name }! Bấm vào liên kết dưới đây để xác minh địa chỉ email:</p>
    <p><a href="{ $url }">{ $url }</a></p>
    <p>Nếu bạn không tạo tài khoản này, bạn có thể bỏ qua email.</p>

## ─── Đặt lại mật khẩu ────────────────────────────────────────────────────────

email-reset-subject = Đặt lại mật khẩu của bạn
email-reset-body =
    <p>Bạn đã yêu cầu đặt lại mật khẩu. Hãy dùng liên kết dưới đây trong vòng một giờ:</p>
    <p><a href="{ $url }">{ $url }</a></p>
    <p>Nếu bạn không yêu cầu điều này, bạn có thể bỏ qua email — mật khẩu của bạn
    sẽ không thay đổi.</p>

## ─── Email thử của quản trị viên ─────────────────────────────────────────────
#
# Chỉ được gửi bởi nút "Send test email" trong /admin/settings, và luôn chỉ gửi
# tới địa chỉ của chính quản trị viên đang thao tác. Khác với các email trên,
# email này không có liên kết nào — nội dung của nó chính là việc nó đã đến.

email-test-subject = Email thử từ { $site_name }
email-test-body =
    <p>Việc gửi email từ { $site_name } đang hoạt động.</p>
    <p>Email này được gửi qua nhà cung cấp <strong>{ $provider }</strong> bằng nút
    "Send test email" trong phần cài đặt quản trị.</p>
