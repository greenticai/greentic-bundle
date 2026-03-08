pub fn lock_matches_manifest(
    lock: &crate::project::BundleLock,
    manifest: &crate::build::manifest::BundleManifest,
) -> bool {
    lock.bundle_id == manifest.bundle_id
        && lock.requested_mode == manifest.requested_mode
        && lock
            .app_packs
            .iter()
            .map(|entry| entry.reference.clone())
            .collect::<Vec<_>>()
            == manifest.app_packs
        && lock
            .extension_providers
            .iter()
            .map(|entry| entry.reference.clone())
            .collect::<Vec<_>>()
            == manifest.extension_providers
        && lock
            .catalogs
            .iter()
            .map(|entry| entry.requested_ref.clone())
            .collect::<Vec<_>>()
            == manifest.catalogs
        && lock.setup_state_files == manifest.generated_setup_files
}
