use std::collections::HashSet;

use ferum_domain::models::role::{RoleGrants, PERMISSIONS, SYSTEM_ROLES};

#[test]
fn permission_keys_are_unique() {
    let mut seen = HashSet::new();
    for def in PERMISSIONS {
        assert!(seen.insert(def.key), "duplicate permission key: {}", def.key);
    }
}

/// A grant list naming a key that no longer exists would silently grant
/// nothing — the seeder joins on the key, so a typo is invisible at runtime.
#[test]
fn every_granted_key_is_defined() {
    let defined: HashSet<&str> = PERMISSIONS.iter().map(|p| p.key).collect();
    for role in SYSTEM_ROLES {
        if let RoleGrants::List(keys) = role.grants {
            for key in keys {
                assert!(defined.contains(key), "role '{}' grants undefined key '{key}'", role.slug);
            }
        }
    }
}

#[test]
fn exactly_one_default_role() {
    assert_eq!(SYSTEM_ROLES.iter().filter(|r| r.is_default).count(), 1);
}
