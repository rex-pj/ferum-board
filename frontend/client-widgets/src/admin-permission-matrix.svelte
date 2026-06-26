<svelte:options customElement={{ tag: "admin-permission-matrix", shadow: "none" }} />

<script lang="ts">
  import { saveRolePermissions } from './lib/admin';

  type Permission = { id: string; key: string; description: string; group_name: string };

  let {
    'role-id': roleId = '',
    permissions = '[]',
    'role-permissions': rolePermissions = '[]',
    'is-system': isSystemRaw = 'false',
  } = $props<{
    'role-id'?: string;
    permissions?: string;
    'role-permissions'?: string;
    'is-system'?: string;
  }>();

  const isSystem: boolean = isSystemRaw === 'true' || isSystemRaw === true as unknown as string;

  let allPerms: Permission[] = $state((() => {
    try { return JSON.parse(permissions); } catch { return []; }
  })());

  let assigned = $state<Set<string>>((() => {
    try { return new Set(JSON.parse(rolePermissions) as string[]); } catch { return new Set(); }
  })());

  let saving = $state(false);
  let saved = $state(false);

  const groups = $derived(
    [...new Set(allPerms.map((p) => p.group_name))].map((g) => ({
      name: g,
      perms: allPerms.filter((p) => p.group_name === g),
    }))
  );

  function toggle(key: string) {
    if (isSystem) return;
    if (assigned.has(key)) {
      assigned.delete(key);
    } else {
      assigned.add(key);
    }
    assigned = new Set(assigned);
  }

  async function save() {
    if (isSystem) return;
    saving = true;
    saved = false;
    const ok = await saveRolePermissions(roleId, [...assigned]);
    saving = false;
    if (ok) {
      saved = true;
      setTimeout(() => (saved = false), 2500);
    }
  }
</script>

{#if isSystem}
  <div class="alert alert-warning d-flex align-items-center gap-2 mb-3" role="alert">
    <i class="fa-solid fa-lock"></i>
    <span>System role permissions are fixed and cannot be changed. They are defined by the platform's seed data.</span>
  </div>
{/if}

<div class="matrix" class:matrix-readonly={isSystem}>
  {#each groups as group}
    <div class="group">
      <div class="group-header">{group.name}</div>
      {#each group.perms as perm}
        <label class="perm-row" class:perm-row-disabled={isSystem}>
          <input
            type="checkbox"
            checked={assigned.has(perm.key)}
            disabled={isSystem}
            onchange={() => toggle(perm.key)}
          />
          <code class="perm-key">{perm.key}</code>
          <span class="perm-desc">{perm.description}</span>
        </label>
      {/each}
    </div>
  {/each}

  {#if !isSystem}
    <div class="save-row">
      <button class="save-btn" onclick={save} disabled={saving}>
        {saving ? 'Saving…' : 'Save Permissions'}
      </button>
      {#if saved}
        <span class="saved-msg"><i class="fa-solid fa-circle-check" aria-hidden="true"></i> Saved</span>
      {/if}
    </div>
  {/if}
</div>

<style>
  .matrix { display: flex; flex-direction: column; gap: 16px; }
  .group { border: 1px solid #dee2e6; border-radius: 8px; overflow: hidden; }
  .group-header {
    padding: 6px 12px; background: var(--bs-secondary-bg, #f8f9fa);
    font-size: 0.7rem; text-transform: uppercase; letter-spacing: 0.05em;
    font-weight: 600; color: #6c757d;
  }
  .perm-row {
    display: flex; align-items: center; gap: 10px; padding: 8px 12px;
    cursor: pointer; border-top: 1px solid #dee2e6; min-height: 44px;
  }
  .perm-row:hover { background: rgba(99,102,241,0.04); }
  .perm-row-disabled { cursor: default; opacity: 0.6; }
  .perm-row-disabled:hover { background: transparent; }
  .matrix-readonly { opacity: 0.75; pointer-events: none; }
  .perm-key { font-size: 0.8rem; color: #6366f1; min-width: 180px; }
  .perm-desc { font-size: 0.85rem; color: #6c757d; }
  .save-row { display: flex; align-items: center; gap: 12px; padding-top: 4px; }
  .save-btn {
    padding: 8px 20px; background: #6366f1; color: #fff;
    border: none; border-radius: 6px; cursor: pointer;
    font-size: 0.875rem; min-height: 44px;
  }
  .save-btn:disabled { opacity: 0.6; cursor: wait; }
  .saved-msg { color: #22c55e; font-size: 0.875rem; font-weight: 600; }
</style>
