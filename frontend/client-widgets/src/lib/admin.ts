import { api } from './api';

export async function saveRolePermissions(
  roleId: string,
  permissionKeys: string[],
): Promise<boolean> {
  const res = await api.put(`/api/admin/roles/${roleId}/permissions`, { permission_keys: permissionKeys });
  return res.ok;
}
