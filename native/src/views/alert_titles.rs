//! English alert titles from upstream XenModel/FriendlyNames.resx.

pub(super) fn title(name: &str) -> &str {
    match name.to_ascii_lowercase().as_str() {
        "alarm" => "Unrecognized alarm",
        "bond_status_changed" => "Bond status has changed",
        "cluster_host_enable_failed" => "Server failed to join the cluster",
        "cluster_host_fencing" => "Server has been fenced",
        "extauth_in_pool_is_non_homogeneous" => {
            "Authentication method is not consistent across the Pool"
        }
        "extauth_init_in_host_failed" => "Active Directory authorization is not available",
        "gfs2_capacity" => "Storage repository low on space",
        "grace_license" => "Grace license issued.",
        "ha_heartbeat_approaching_timeout" => "HA heartbeat approaching timeout",
        "ha_host_failed" => "Server failed",
        "ha_host_was_fenced" => "Server recovered",
        "ha_network_bonding_error" => "HA network bonding error",
        "ha_pool_drop_in_plan_exists_for" => "Drop in HA failover tolerance",
        "ha_pool_overcommitted" => "Drop in HA failover tolerance",
        "ha_protected_vm_restart_failed" => "HA-protected VM failed to restart",
        "ha_statefile_approaching_timeout" => "Heartbeat SR approaching timeout",
        "ha_statefile_lost" => "Heartbeat SR lost",
        "ha_xapi_healthcheck_approaching_timeout" => "Server healthcheck approaching timeout",
        "host_clock_skew_detected" => "Server clock not synchronized",
        "host_clock_went_backwards" => "Server clock has gone backwards",
        "host_cpu_features_down" => "Host CPU features reduced",
        "host_cpu_features_up" => "Host CPU features increased",
        "host_sync_data_failed" => "Server statistics synchronization failed",
        "ip_configured_pif_can_unplug" => "Network configuration issue",
        "license_does_not_support_pooling" => "License does not support pooling",
        "license_expired" => "License Expired.",
        "license_expires_soon" => "License expires soon",
        "license_not_available" => "A license was not available.",
        "license_server_connected" => "License server connection successful.",
        "license_server_unavailable" => "Server disconnected from license server.",
        "license_server_unreachable" => "The license server could not be reached.",
        "license_server_version_obsolete" => "The license server is an out-of-date version.",
        "metadata_lun_broken" => "Lost access to HA metadata disk",
        "metadata_lun_healthy" => "Restored access to HA metadata disk",
        "multipath_periodic_alert" => "Multipath Alert",
        "pbd_plug_failed_on_server_start" => "Failed to attach storage on server start",
        "pool_cpu_features_down" => "Pool CPU features reduced",
        "pool_cpu_features_up" => "Pool CPU features increased",
        "pool_master_transition" => "Pool coordinator changed",
        "pvs_proxy_no_cache_sr_available" => "No cache storage available for PVS site",
        "pvs_proxy_setup_failed" => "PVS proxy setup failed",
        "srunavailable" => "Cannot reclaim freed space because the SR is already busy",
        "tls_verification_emergency_disabled" => "Certificate verification is disabled.",
        "unknowntrimexception" => "An unknown error occurred when reclaiming freed space",
        "unsupportedsrfortrim" => "Reclaiming freed space not supported on this SR",
        "vbd_qos_failed" => "VM Disk QoS Failed",
        "vcpu_qos_failed" => "VM vCPU QoS Failed",
        "vdi_cbt_metadata_inconsistent" => {
            "Changed block tracking disabled due to inconsistent metadata"
        }
        "vdi_cbt_resize_failed" => {
            "Changed block tracking disabled due to a virtual disk resizing failure"
        }
        "vdi_cbt_snapshot_failed" => "Changed block tracking disabled due to a snapshot failure",
        "vif_qos_failed" => "VM Network Interface QoS Failed",
        "vm_checkpointed" => "Snapshot of disks and memory for virtual machine has been created",
        "vm_cloned" => "VM Cloned",
        "vm_crashed" => "VM Crashed",
        "vm_migrated" => "Virtual machine has been migrated",
        "vm_paused" => "Virtual machine has been paused",
        "vm_rebooted" => "VM Rebooted",
        "vm_resumed" => "VM Resumed",
        "vm_secure_boot_failed" => "VM Secure Boot verification failed",
        "vm_shutdown" => "VM Shut down",
        "vm_snapshot_reverted" => "Virtual machine has been reverted to snapshot",
        "vm_snapshotted" => "Snapshot of disks for virtual machine has been created",
        "vm_started" => "VM Started",
        "vm_suspended" => "VM Suspended",
        "vm_uncooperative" => "VM Uncooperative",
        "vm_unpaused" => "Virtual machine has been unpaused",
        "vmss_license_error" => {
            "Snapshot schedule failed: insufficient license for Scheduled snapshots"
        }
        "vmss_snapshot_failed" => "Snapshot schedule failed",
        "vmss_snapshot_lock_failed" => "Snapshot schedule failed",
        "vmss_snapshot_missed_event" => "Snapshot schedule failed",
        "vmss_snapshot_succeeded" => "Snapshot schedule succeeded",
        "vmss_xapi_logon_failure" => "Snapshot schedule XAPI logon failure",
        "wlb_consultation_failed" => "WLB consultation has failed",
        "wlb_host_power_off" => "WLB Host Power Off",
        "wlb_host_power_on" => "WLB Host Power On",
        "wlb_optimization_alert" => "WLB optimization alert",
        "wlb_server_time_discrepancy" => "WLB Server Time Discrepancy",
        "wlb_vm_relocation" => "WLB VM Relocation",
        _ => name,
    }
}
