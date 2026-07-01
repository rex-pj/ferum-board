document.addEventListener("DOMContentLoaded", function () {
  const btn    = document.getElementById("audit-ref-btn");
  const tpl    = document.getElementById("audit-action-ref");
  const qInput = document.getElementById("audit-q");
  if (!btn || !tpl || !qInput) return;

  const popover = new bootstrap.Popover(btn, {
    html:        true,
    sanitize:    false,
    trigger:     "manual",
    placement:   "auto",
    content:     tpl.innerHTML,
    container:   "body",
    customClass: "audit-log-popover",
  });

  btn.addEventListener("click", function (e) {
    e.stopPropagation();
    popover.toggle();
  });

  document.addEventListener("click", function (e) {
    if (btn.contains(e.target) || e.target.closest(".audit-log-popover")) return;

    popover.hide();

    const chip = e.target.closest(".audit-chip");
    if (!chip) return;
    qInput.value = chip.dataset.action;
    qInput.closest("form").submit();
  });
});
