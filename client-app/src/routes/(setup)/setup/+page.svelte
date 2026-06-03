<script lang="ts">
  import { enhance } from "$app/forms";
  import { goto } from "$app/navigation";
  import { ROUTES } from "$lib/routes";

  let { form }: { form: any } = $props();

  // ─── Step state ───────────────────────────────────────────────────────────
  let step = $state(1);
  const TOTAL_STEPS = 4;

  // ─── Form data ────────────────────────────────────────────────────────────
  let admin = $state({ username: "", email: "", password: "", confirm: "" });
  let config = $state({
    site_name: "",
    site_tagline: "",
    primary_color: "#0d6efd",
    registration_open: true,
    smtp_host: "",
    smtp_port: "587",
    smtp_user: "",
    smtp_pass: "",
  });
  let seedExampleData = $state(true);
  let showSmtp = $state(false);
  let submitting = $state(false);

  // ─── Validation helpers ───────────────────────────────────────────────────
  function isValidUsername(u: string): boolean {
    if (u.length < 3 || u.length > 30) return false;
    return /^[a-zA-Z0-9_-]+$/.test(u);
  }

  let adminErrors = $derived({
    username:
      admin.username.length > 0 && !isValidUsername(admin.username)
        ? "Username must be 3–30 chars, alphanumeric/underscore/hyphen"
        : "",
    passwordMismatch:
      admin.confirm.length > 0 && admin.password !== admin.confirm
        ? "Passwords do not match"
        : "",
    passwordTooShort:
      admin.password.length > 0 && admin.password.length < 8
        ? "Password must be at least 8 characters"
        : "",
  });

  let step2Valid = $derived(
    isValidUsername(admin.username) &&
      admin.email.includes("@") &&
      admin.password.length >= 8 &&
      admin.password === admin.confirm,
  );

  // ─── Build the submission payload ─────────────────────────────────────────
  function buildPayload() {
    const hasConfig =
      config.site_name.trim() ||
      config.site_tagline.trim() ||
      config.primary_color !== "#0d6efd" ||
      !config.registration_open ||
      config.smtp_host.trim();

    const payload: Record<string, unknown> = {
      admin_username: admin.username,
      admin_email: admin.email,
      admin_password: admin.password,
      seed_example_data: seedExampleData,
    };

    if (hasConfig) {
      payload.config = {
        ...(config.site_name.trim() && { site_name: config.site_name }),
        ...(config.site_tagline.trim() && {
          site_tagline: config.site_tagline,
        }),
        ...(config.primary_color && { primary_color: config.primary_color }),
        registration_open: config.registration_open,
        ...(config.smtp_host.trim() && { smtp_host: config.smtp_host }),
        ...(config.smtp_port && { smtp_port: parseInt(config.smtp_port) }),
        ...(config.smtp_user.trim() && { smtp_user: config.smtp_user }),
        ...(config.smtp_pass && { smtp_pass: config.smtp_pass }),
      };
    }

    return JSON.stringify(payload);
  }
</script>

<svelte:head>
  <title>Installation | Ferum Board</title>
  <meta name="robots" content="noindex" />
</svelte:head>

<!-- ─── Progress indicator ──────────────────────────────────────────────────── -->
{#if step < 5}
  <div class="d-flex align-items-center gap-2 mb-4">
    {#each Array.from({ length: TOTAL_STEPS }, (_, i) => i + 1) as s}
      <div
        class="rounded-pill flex-grow-1"
        style="height: 4px; background-color: {s <= step
          ? 'var(--bs-primary)'
          : 'var(--bs-border-color)'};"
      ></div>
    {/each}
    <small class="text-muted text-nowrap ms-1">Step {step}/{TOTAL_STEPS}</small>
  </div>
{/if}

<!-- ─── Step 1: Welcome ───────────────────────────────────────────────────────── -->
{#if step === 1}
  <div class="text-center mb-4">
    <div class="text-success mb-3 step-icon">
      <i class="fa-solid fa-circle-check"></i>
    </div>
    <h2 class="h5 mb-1">Database schema ready</h2>
    <p class="text-muted small mb-0">
      All database tables and migrations have been applied successfully.
    </p>
  </div>

  <div class="alert alert-info d-flex align-items-start gap-2 mb-4">
    <i class="fa-solid fa-circle-info mt-1 flex-shrink-0"></i>
    <div>
      This wizard will create your admin account and configure the forum. It
      runs only once — after setup is complete, these pages are permanently
      disabled.
    </div>
  </div>

  <button class="btn btn-primary w-100" onclick={() => (step = 2)}>
    Get Started <i class="fa-solid fa-arrow-right ms-1"></i>
  </button>

  <!-- ─── Step 2: Admin account ─────────────────────────────────────────────────── -->
{:else if step === 2}
  <h2 class="h5 mb-1">Create admin account</h2>
  <p class="text-muted small mb-4">
    This will be the primary administrator of your forum.
  </p>

  <div class="mb-3">
    <label for="username" class="form-label">Username</label>
    <input
      type="text"
      id="username"
      class="form-control"
      class:is-invalid={adminErrors.username}
      bind:value={admin.username}
      autocomplete="username"
      minlength="3"
      maxlength="30"
      pattern={"[a-zA-Z0-9_\\-]+"}
    />
    <div class="form-text">
      3–30 characters. Letters, numbers, _ and - only.
    </div>
    {#if adminErrors.username}
      <div class="invalid-feedback">{adminErrors.username}</div>
    {/if}
  </div>

  <div class="mb-3">
    <label for="email" class="form-label">Email address</label>
    <input
      type="email"
      id="email"
      class="form-control"
      bind:value={admin.email}
      autocomplete="email"
    />
  </div>

  <div class="mb-3">
    <label for="password" class="form-label">Password</label>
    <input
      type="password"
      id="password"
      class="form-control"
      class:is-invalid={adminErrors.passwordTooShort}
      bind:value={admin.password}
      autocomplete="new-password"
      minlength="8"
    />
    {#if adminErrors.passwordTooShort}
      <div class="invalid-feedback">{adminErrors.passwordTooShort}</div>
    {/if}
  </div>

  <div class="mb-4">
    <label for="confirm" class="form-label">Confirm password</label>
    <input
      type="password"
      id="confirm"
      class="form-control"
      class:is-invalid={adminErrors.passwordMismatch}
      bind:value={admin.confirm}
      autocomplete="new-password"
    />
    {#if adminErrors.passwordMismatch}
      <div class="invalid-feedback">{adminErrors.passwordMismatch}</div>
    {/if}
  </div>

  <div class="d-flex gap-2">
    <button class="btn btn-outline-secondary" onclick={() => (step = 1)}>
      <i class="fa-solid fa-arrow-left me-1"></i> Back
    </button>
    <button
      class="btn btn-primary flex-grow-1"
      disabled={!step2Valid}
      onclick={() => (step = 3)}
    >
      Next <i class="fa-solid fa-arrow-right ms-1"></i>
    </button>
  </div>

  <!-- ─── Step 3: Site configuration ────────────────────────────────────────────── -->
{:else if step === 3}
  <h2 class="h5 mb-1">Site configuration</h2>
  <p class="text-muted small mb-4">
    Customise your forum. You can change these later in the admin settings.
  </p>

  <div class="mb-3">
    <label for="site_name" class="form-label">Site name</label>
    <input
      type="text"
      id="site_name"
      class="form-control"
      bind:value={config.site_name}
      placeholder="Ferum Board"
    />
  </div>

  <div class="mb-3">
    <label for="site_tagline" class="form-label">Tagline</label>
    <input
      type="text"
      id="site_tagline"
      class="form-control"
      bind:value={config.site_tagline}
      placeholder="A modern self-hosted forum"
    />
  </div>

  <div class="row mb-3">
    <div class="col-md-6">
      <label for="primary_color" class="form-label">Primary colour</label>
      <div class="input-group">
        <input
          type="color"
          id="primary_color"
          class="form-control form-control-color color-swatch"
          bind:value={config.primary_color}
        />
        <input
          type="text"
          class="form-control"
          bind:value={config.primary_color}
        />
      </div>
    </div>
    <div class="col-md-6 d-flex align-items-end">
      <div
        class="form-check form-switch mb-0 mt-3 mt-md-0 reg-switch"
      >
        <input
          class="form-check-input"
          type="checkbox"
          id="registration_open"
          bind:checked={config.registration_open}
        />
        <label class="form-check-label" for="registration_open"
          >Open registration</label
        >
      </div>
    </div>
  </div>

  <button
    type="button"
    class="btn btn-link ps-0 mb-3 text-decoration-none"
    onclick={() => (showSmtp = !showSmtp)}
  >
    <i class="fa-solid {showSmtp ? 'fa-chevron-down' : 'fa-chevron-right'} me-1"
    ></i>
    Email / SMTP settings
  </button>

  {#if showSmtp}
    <div class="border rounded p-3 mb-3 bg-body-tertiary">
      <div class="row g-3">
        <div class="col-md-8">
          <label class="form-label" for="smtp_host">SMTP host</label>
          <input
            type="text"
            id="smtp_host"
            class="form-control"
            bind:value={config.smtp_host}
            placeholder="smtp.example.com"
          />
        </div>
        <div class="col-md-4">
          <label class="form-label" for="smtp_port">Port</label>
          <input
            type="number"
            id="smtp_port"
            class="form-control"
            bind:value={config.smtp_port}
            min="1"
            max="65535"
          />
        </div>
        <div class="col-md-6">
          <label class="form-label" for="smtp_user">Username</label>
          <input
            type="text"
            id="smtp_user"
            class="form-control"
            bind:value={config.smtp_user}
            autocomplete="off"
          />
        </div>
        <div class="col-md-6">
          <label class="form-label" for="smtp_pass">Password</label>
          <input
            type="password"
            id="smtp_pass"
            class="form-control"
            bind:value={config.smtp_pass}
            autocomplete="new-password"
          />
        </div>
      </div>
    </div>
  {/if}

  <div class="d-flex gap-2">
    <button class="btn btn-outline-secondary" onclick={() => (step = 2)}>
      <i class="fa-solid fa-arrow-left me-1"></i> Back
    </button>
    <button class="btn btn-outline-primary" onclick={() => (step = 4)}
      >Skip</button
    >
    <button class="btn btn-primary flex-grow-1" onclick={() => (step = 4)}>
      Next <i class="fa-solid fa-arrow-right ms-1"></i>
    </button>
  </div>

  <!-- ─── Step 4: Example data + final submit ───────────────────────────────────── -->
{:else if step === 4}
  <h2 class="h5 mb-1">Example content</h2>
  <p class="text-muted small mb-4">
    Optionally install sample categories, threads, posts, and reactions to
    explore the forum.
  </p>

  <div
    class="border rounded p-4 mb-4"
    style="cursor: pointer; background-color: {seedExampleData
      ? 'var(--bs-primary-bg-subtle)'
      : ''}; border-color: {seedExampleData
      ? 'var(--bs-primary)'
      : ''} !important;"
    role="button"
    tabindex="0"
    onclick={() => (seedExampleData = !seedExampleData)}
    onkeydown={(e) => e.key === "Enter" && (seedExampleData = !seedExampleData)}
  >
    <div class="d-flex align-items-start gap-3">
      <input
        type="checkbox"
        class="form-check-input mt-1 flex-shrink-0 seed-checkbox"
        bind:checked={seedExampleData}
        onclick={(e) => e.stopPropagation()}
      />
      <div>
        <div class="fw-semibold mb-1">Install example content</div>
        <div class="text-muted small">
          Creates 3 categories (Announcements, General, Feedback) with a sample
          thread, post, and reaction in each — great for exploring the interface
          before adding real content.
        </div>
      </div>
    </div>
  </div>

  {#if form?.error}
    <div class="alert alert-danger mb-3">{form.error}</div>
  {/if}

  <form
    method="POST"
    action="?/run_setup"
    use:enhance={() => {
      submitting = true;
      return async ({ result, update }) => {
        submitting = false;
        if (result.type === "success") {
          step = 5;
          setTimeout(() => goto(ROUTES.ADMIN.DASHBOARD), 2000);
        } else {
          await update();
        }
      };
    }}
  >
    <input type="hidden" name="payload" value={buildPayload()} />

    <div class="d-flex gap-2">
      <button
        type="button"
        class="btn btn-outline-secondary"
        onclick={() => (step = 3)}
        disabled={submitting}
      >
        <i class="fa-solid fa-arrow-left me-1"></i> Back
      </button>
      <button
        type="submit"
        class="btn btn-primary flex-grow-1"
        disabled={submitting}
      >
        {#if submitting}
          <span
            class="spinner-border spinner-border-sm me-2"
            role="status"
            aria-hidden="true"
          ></span>
          Installing…
        {:else}
          <i class="fa-solid fa-rocket me-1"></i> Complete Installation
        {/if}
      </button>
    </div>
  </form>

  <!-- ─── Step 5: Done ──────────────────────────────────────────────────────────── -->
{:else if step === 5}
  <div class="text-center py-3">
    <div class="text-success mb-3 done-icon">
      <i class="fa-solid fa-circle-check"></i>
    </div>
    <h2 class="h5 mb-2">Installation complete!</h2>
    <p class="text-muted mb-4">
      You are now signed in as administrator. Redirecting to the dashboard…
    </p>
    <div class="spinner-border text-primary" role="status">
      <span class="visually-hidden">Redirecting…</span>
    </div>
  </div>
{/if}

<style>
  .step-icon  { font-size: 3rem; }
  .done-icon  { font-size: 3.5rem; }
  .color-swatch { max-width: 3rem; }
  .reg-switch { min-height: 2.5rem; padding-top: 0.5rem; }
  .seed-checkbox { min-width: 1.25rem; min-height: 1.25rem; }
</style>
