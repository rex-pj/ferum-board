//! What a transactional email is allowed to say, and which values it may name.
//!
//! Single source of truth, read by the seeder and by the save-time validator —
//! the same shape `PERMISSIONS` uses. Adding an email is an entry in
//! [`EMAIL_TEMPLATES`] plus one in [`EMAIL_TEMPLATE_DEFAULTS`] per locale; there
//! is no migration and no SQL literal that could drift from the key the sender
//! passes.
//!
//! The copy lives here rather than in `locales/*/emails.ftl` because the
//! database is authoritative at runtime: these are what the seeder writes on a
//! fresh install and what "restore default" restores to. One consequence, stated
//! rather than discovered: a translator can no longer contribute email copy by
//! editing a `.ftl`, and must edit this file or the admin UI.

/// How a value is treated on its way into the rendered output.
///
/// **The escaping policy is per variable, and that is the whole reason this
/// catalogue exists.** A single blanket rule cannot be right for both a thread
/// title and an href.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VarKind {
    /// Author- or admin-controlled prose. HTML-escaped into the body.
    ///
    /// Never trust the source: `validate_thread_title` checks length only and a
    /// title never passes through ammonia, so an unescaped one puts a working
    /// `<a href>` in a stranger's inbox over our own verified sending domain.
    Text,
    /// A link the application built. Scheme-checked, then attribute-escaped.
    Url,
    /// Already-rendered HTML, interpolated verbatim.
    ///
    /// **Only the layout's content slot may use this.** Escaping there would
    /// show the recipient the markup of their own email.
    Raw,
}

/// One value a template may name.
pub struct VarDef {
    pub name: &'static str,
    pub kind: VarKind,
    /// Whether the **body** must contain this placeholder, enforced on save.
    ///
    /// Subjects have no required variables: a subject naming
    /// `unsubscribe_url` would be nonsense, and requiring `url` in one would
    /// forbid the perfectly good "Verify your email".
    pub required_in_body: bool,
    /// Stand-in shown by the admin preview.
    ///
    /// Lives beside the declaration so the variable names exist in exactly one
    /// place. A preview that built its own samples would be a second list to
    /// keep in step, and the first one to fall behind.
    pub sample: &'static str,
}

/// One editable email.
pub struct EmailTemplateDef {
    pub key: &'static str,
    /// Shown above the editor. This is the only place the operator learns what
    /// triggers the message, so say that rather than restating the name.
    pub description: &'static str,
    pub vars: &'static [VarDef],
}

/// The wrapper every message body is rendered into.
///
/// Its own key so it is editable like any other, and excluded from the list of
/// *messages* because nothing sends it on its own.
pub const LAYOUT_KEY: &str = "email-layout";

const NOTIFY_VARS: &[VarDef] = &[
    VarDef { name: "site_name",       kind: VarKind::Text, required_in_body: false, sample: "Ferum Board" },
    VarDef { name: "actor",           kind: VarKind::Text, required_in_body: false, sample: "alice" },
    // Carries markup on purpose: the preview is the one place an admin can see
    // that an author-controlled title arrives escaped rather than rendered.
    VarDef { name: "thread_title",    kind: VarKind::Text, required_in_body: false, sample: "Sofa <b>hay nhất</b> 2026?" },
    VarDef { name: "url",             kind: VarKind::Url,  required_in_body: true,  sample: "https://example.test/forum/t/a-thread" },
    // A notification with no working opt-out is the message people report as
    // spam rather than mute, and that cost lands on the sending domain.
    VarDef { name: "unsubscribe_url", kind: VarKind::Url,  required_in_body: true,  sample: "https://example.test/unsubscribe/TOKEN" },
    VarDef { name: "settings_url",    kind: VarKind::Url,  required_in_body: false, sample: "https://example.test/account" },
];

pub const EMAIL_TEMPLATES: &[EmailTemplateDef] = &[
    EmailTemplateDef {
        key: "email-verify",
        description: "Sent on registration and whenever a member asks for a new verification link.",
        vars: &[
            VarDef { name: "site_name", kind: VarKind::Text, required_in_body: false, sample: "Ferum Board" },
            VarDef { name: "url",       kind: VarKind::Url,  required_in_body: true,  sample: "https://example.test/verify-email/TOKEN" },
        ],
    },
    EmailTemplateDef {
        key: "email-reset",
        description: "Sent when a password reset is requested. The link is valid for one hour.",
        vars: &[
            VarDef { name: "site_name", kind: VarKind::Text, required_in_body: false, sample: "Ferum Board" },
            VarDef { name: "url",       kind: VarKind::Url,  required_in_body: true,  sample: "https://example.test/reset-password?token=TOKEN" },
        ],
    },
    EmailTemplateDef {
        key: "email-notify-reply",
        description: "Sent to a thread author when someone replies, if they opted in.",
        vars: NOTIFY_VARS,
    },
    EmailTemplateDef {
        key: "email-notify-mention",
        description: "Sent when someone @-mentions a member, if they opted in.",
        vars: NOTIFY_VARS,
    },
    EmailTemplateDef {
        key: "email-test",
        description: "Sent only by the \"Send test email\" button, only to the acting admin.",
        vars: &[
            VarDef { name: "site_name", kind: VarKind::Text, required_in_body: false, sample: "Ferum Board" },
            VarDef { name: "provider",  kind: VarKind::Text, required_in_body: false, sample: "SMTP" },
        ],
    },
    EmailTemplateDef {
        key: LAYOUT_KEY,
        description: "Wraps every message above. `content` is the rendered message body.",
        vars: &[
            VarDef { name: "site_name", kind: VarKind::Text, required_in_body: false, sample: "Ferum Board" },
            VarDef { name: "content",   kind: VarKind::Raw,  required_in_body: true,  sample: "<p>The message body appears here.</p>" },
        ],
    },
];

/// The copy a fresh install starts with, and what "restore default" restores.
pub struct EmailTemplateDefault {
    pub key: &'static str,
    /// Canonical locale tag, exactly as `Locale::parse` produces it.
    pub locale: &'static str,
    pub subject: &'static str,
    pub body_html: &'static str,
}

/// Looks up a template definition by key.
pub fn template_def(key: &str) -> Option<&'static EmailTemplateDef> {
    EMAIL_TEMPLATES.iter().find(|t| t.key == key)
}

/// The default copy for `key` in `locale`, if this locale ships one.
pub fn template_default(key: &str, locale: &str) -> Option<&'static EmailTemplateDefault> {
    EMAIL_TEMPLATE_DEFAULTS
        .iter()
        .find(|d| d.key == key && d.locale == locale)
}

pub const EMAIL_TEMPLATE_DEFAULTS: &[EmailTemplateDefault] = &[
    // ─── en ───────────────────────────────────────────────────────────────────
    EmailTemplateDefault {
        key: "email-verify",
        locale: "en",
        subject: "Verify your email",
        body_html: "<p>Welcome to {{ site_name }}! Click the link below to verify your email address:</p>\n<p><a href=\"{{ url }}\">{{ url }}</a></p>\n<p>If you didn't create an account, you can ignore this email.</p>",
    },
    EmailTemplateDefault {
        key: "email-reset",
        locale: "en",
        subject: "Reset your password",
        body_html: "<p>You asked to reset your password. Use the link below within one hour:</p>\n<p><a href=\"{{ url }}\">{{ url }}</a></p>\n<p>If you didn't request this, you can safely ignore this email — your password will not change.</p>",
    },
    EmailTemplateDefault {
        key: "email-notify-reply",
        locale: "en",
        subject: "{{ actor }} replied to \"{{ thread_title }}\"",
        body_html: "<p><strong>{{ actor }}</strong> replied to your thread <strong>{{ thread_title }}</strong> on {{ site_name }}.</p>\n<p><a href=\"{{ url }}\">Read the reply</a></p>\n<hr>\n<p style=\"font-size:12px;color:#666\">\n  <a href=\"{{ unsubscribe_url }}\">Unsubscribe from these emails</a> ·\n  <a href=\"{{ settings_url }}\">Notification settings</a>\n</p>",
    },
    EmailTemplateDefault {
        key: "email-notify-mention",
        locale: "en",
        subject: "{{ actor }} mentioned you in \"{{ thread_title }}\"",
        body_html: "<p><strong>{{ actor }}</strong> mentioned you in <strong>{{ thread_title }}</strong> on {{ site_name }}.</p>\n<p><a href=\"{{ url }}\">View the post</a></p>\n<hr>\n<p style=\"font-size:12px;color:#666\">\n  <a href=\"{{ unsubscribe_url }}\">Unsubscribe from these emails</a> ·\n  <a href=\"{{ settings_url }}\">Notification settings</a>\n</p>",
    },
    EmailTemplateDefault {
        key: "email-test",
        locale: "en",
        subject: "Test email from {{ site_name }}",
        body_html: "<p>Mail delivery from {{ site_name }} is working.</p>\n<p>This message was sent through the <strong>{{ provider }}</strong> provider by the \"Send test email\" button in the admin settings.</p>",
    },
    EmailTemplateDefault {
        key: LAYOUT_KEY,
        locale: "en",
        // Subject is unused for the layout; kept non-empty so the column needs
        // no special case and the editor shows why it is inert.
        subject: "(layout — the message supplies the subject)",
        body_html: "<!doctype html>\n<html>\n<head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width\"></head>\n<body style=\"margin:0;padding:24px;background:#f6f7f9;font-family:-apple-system,Segoe UI,Roboto,Helvetica,Arial,sans-serif;color:#1f2328\">\n  <div style=\"max-width:560px;margin:0 auto;background:#ffffff;border-radius:8px;padding:24px\">\n    <p style=\"margin:0 0 16px;font-size:18px;font-weight:600\">{{ site_name }}</p>\n    {{ content }}\n  </div>\n</body>\n</html>",
    },
    // ─── vi ───────────────────────────────────────────────────────────────────
    EmailTemplateDefault {
        key: "email-verify",
        locale: "vi",
        subject: "Xác minh địa chỉ email của bạn",
        body_html: "<p>Chào mừng bạn đến với {{ site_name }}! Bấm vào liên kết dưới đây để xác minh địa chỉ email:</p>\n<p><a href=\"{{ url }}\">{{ url }}</a></p>\n<p>Nếu bạn không tạo tài khoản này, bạn có thể bỏ qua email.</p>",
    },
    EmailTemplateDefault {
        key: "email-reset",
        locale: "vi",
        subject: "Đặt lại mật khẩu của bạn",
        body_html: "<p>Bạn đã yêu cầu đặt lại mật khẩu. Hãy dùng liên kết dưới đây trong vòng một giờ:</p>\n<p><a href=\"{{ url }}\">{{ url }}</a></p>\n<p>Nếu bạn không yêu cầu điều này, bạn có thể bỏ qua email — mật khẩu của bạn sẽ không thay đổi.</p>",
    },
    EmailTemplateDefault {
        key: "email-notify-reply",
        locale: "vi",
        subject: "{{ actor }} đã trả lời \"{{ thread_title }}\"",
        body_html: "<p><strong>{{ actor }}</strong> đã trả lời chủ đề <strong>{{ thread_title }}</strong> của bạn trên {{ site_name }}.</p>\n<p><a href=\"{{ url }}\">Xem câu trả lời</a></p>\n<hr>\n<p style=\"font-size:12px;color:#666\">\n  <a href=\"{{ unsubscribe_url }}\">Ngừng nhận các email này</a> ·\n  <a href=\"{{ settings_url }}\">Cài đặt thông báo</a>\n</p>",
    },
    EmailTemplateDefault {
        key: "email-notify-mention",
        locale: "vi",
        subject: "{{ actor }} đã nhắc tới bạn trong \"{{ thread_title }}\"",
        body_html: "<p><strong>{{ actor }}</strong> đã nhắc tới bạn trong <strong>{{ thread_title }}</strong> trên {{ site_name }}.</p>\n<p><a href=\"{{ url }}\">Xem bài viết</a></p>\n<hr>\n<p style=\"font-size:12px;color:#666\">\n  <a href=\"{{ unsubscribe_url }}\">Ngừng nhận các email này</a> ·\n  <a href=\"{{ settings_url }}\">Cài đặt thông báo</a>\n</p>",
    },
    EmailTemplateDefault {
        key: "email-test",
        locale: "vi",
        subject: "Email thử từ {{ site_name }}",
        body_html: "<p>Việc gửi email từ {{ site_name }} đang hoạt động.</p>\n<p>Email này được gửi qua nhà cung cấp <strong>{{ provider }}</strong> bằng nút \"Send test email\" trong phần cài đặt quản trị.</p>",
    },
    EmailTemplateDefault {
        key: LAYOUT_KEY,
        locale: "vi",
        subject: "(bố cục — tiêu đề do từng email cung cấp)",
        body_html: "<!doctype html>\n<html>\n<head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width\"></head>\n<body style=\"margin:0;padding:24px;background:#f6f7f9;font-family:-apple-system,Segoe UI,Roboto,Helvetica,Arial,sans-serif;color:#1f2328\">\n  <div style=\"max-width:560px;margin:0 auto;background:#ffffff;border-radius:8px;padding:24px\">\n    <p style=\"margin:0 0 16px;font-size:18px;font-weight:600\">{{ site_name }}</p>\n    {{ content }}\n  </div>\n</body>\n</html>",
    },
];

/// A stored template, as the repository returns it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmailTemplate {
    pub key: String,
    pub locale: String,
    pub subject: String,
    pub body_html: String,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}
