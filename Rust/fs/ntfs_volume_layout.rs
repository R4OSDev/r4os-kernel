//! Private C layout assertions compiled in the product and original host owner archive.
const _: () = {
    assert!(core::mem::size_of::<super::ntfs_format_types::View>() == 16);
    assert!(core::mem::align_of::<super::ntfs_format_types::View>() == 8);
    assert!(core::mem::offset_of!(super::ntfs_format_types::View, data) == 0);
    assert!(core::mem::offset_of!(super::ntfs_format_types::View, len) == 8);
    assert!(core::mem::size_of::<super::ntfs_format_types::Buffer>() == 16);
    assert!(core::mem::align_of::<super::ntfs_format_types::Buffer>() == 8);
    assert!(core::mem::offset_of!(super::ntfs_format_types::Buffer, data) == 0);
    assert!(core::mem::offset_of!(super::ntfs_format_types::Buffer, len) == 8);
    assert!(core::mem::size_of::<super::ntfs_format_types::Run>() == 24);
    assert!(core::mem::align_of::<super::ntfs_format_types::Run>() == 8);
    assert!(core::mem::offset_of!(super::ntfs_format_types::Run, length_clusters) == 0);
    assert!(core::mem::offset_of!(super::ntfs_format_types::Run, has_lcn) == 8);
    assert!(core::mem::offset_of!(super::ntfs_format_types::Run, lcn) == 16);
    assert!(core::mem::size_of::<super::ntfs_format_types::FileReference>() == 16);
    assert!(core::mem::align_of::<super::ntfs_format_types::FileReference>() == 8);
    assert!(core::mem::offset_of!(super::ntfs_format_types::FileReference, record) == 0);
    assert!(core::mem::offset_of!(super::ntfs_format_types::FileReference, sequence) == 8);
    assert!(core::mem::size_of::<super::ntfs_format_types::FileRecordHeader>() == 56);
    assert!(core::mem::align_of::<super::ntfs_format_types::FileRecordHeader>() == 8);
    assert!(core::mem::offset_of!(super::ntfs_format_types::FileRecordHeader, magic) == 0);
    assert!(core::mem::offset_of!(super::ntfs_format_types::FileRecordHeader, lsn) == 8);
    assert!(core::mem::offset_of!(super::ntfs_format_types::FileRecordHeader, sequence) == 16);
    assert!(core::mem::offset_of!(super::ntfs_format_types::FileRecordHeader, link_count) == 18);
    assert!(core::mem::offset_of!(super::ntfs_format_types::FileRecordHeader, attrs_offset) == 20);
    assert!(core::mem::offset_of!(super::ntfs_format_types::FileRecordHeader, flags) == 22);
    assert!(core::mem::offset_of!(super::ntfs_format_types::FileRecordHeader, bytes_in_use) == 24);
    assert!(
        core::mem::offset_of!(super::ntfs_format_types::FileRecordHeader, bytes_allocated) == 28
    );
    assert!(core::mem::offset_of!(super::ntfs_format_types::FileRecordHeader, base_record) == 32);
    assert!(
        core::mem::offset_of!(
            super::ntfs_format_types::FileRecordHeader,
            next_attr_instance
        ) == 48
    );
    assert!(core::mem::offset_of!(super::ntfs_format_types::FileRecordHeader, record_number) == 52);
    assert!(core::mem::size_of::<super::ntfs_volume_types::WorkStats>() == 32);
    assert!(core::mem::align_of::<super::ntfs_volume_types::WorkStats>() == 8);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::WorkStats, cluster_bitmap_probes) == 0);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::WorkStats, record_bitmap_probes) == 8);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::WorkStats, read_zero_bytes) == 16);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::WorkStats, directory_entries) == 24);
    assert!(core::mem::size_of::<super::ntfs_volume_types::Entry>() == 832);
    assert!(core::mem::align_of::<super::ntfs_volume_types::Entry>() == 8);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::Entry, name) == 0);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::Entry, name_len) == 768);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::Entry, attr) == 776);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::Entry, reparse) == 777);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::Entry, record) == 784);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::Entry, sequence) == 792);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::Entry, size) == 800);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::Entry, created_time_nt) == 808);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::Entry, modified_time_nt) == 816);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::Entry, access_time_nt) == 824);
    assert!(core::mem::size_of::<super::ntfs_volume_types::LookupResult>() == 872);
    assert!(core::mem::align_of::<super::ntfs_volume_types::LookupResult>() == 8);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::LookupResult, record) == 0);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::LookupResult, sequence) == 8);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::LookupResult, entry) == 16);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::LookupResult, in_root) == 848);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::LookupResult, block_vcn) == 856);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::LookupResult, entry_offset) == 864);
    assert!(core::mem::size_of::<super::ntfs_volume_types::AttrScratch>() == 7216);
    assert!(core::mem::align_of::<super::ntfs_volume_types::AttrScratch>() == 8);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::AttrScratch, runs) == 0);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::AttrScratch, count) == 6144);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::AttrScratch, data_size) == 6152);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::AttrScratch, initialized_size) == 6160);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::AttrScratch, alloc_size) == 6168);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::AttrScratch, flags) == 6176);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::AttrScratch, compression_unit) == 6178);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::AttrScratch, resident) == 6179);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::AttrScratch, resident_len) == 6184);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::AttrScratch, resident_copy) == 6192);
    assert!(core::mem::size_of::<super::ntfs_volume_types::Geometry>() == 16);
    assert!(core::mem::align_of::<super::ntfs_volume_types::Geometry>() == 8);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::Geometry, partition_lba) == 0);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::Geometry, cluster_bytes) == 4);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::Geometry, total_sectors) == 8);
    assert!(core::mem::size_of::<super::ntfs_volume_types::DirectoryFrame>() == 16);
    assert!(core::mem::align_of::<super::ntfs_volume_types::DirectoryFrame>() == 8);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::DirectoryFrame, vcn) == 0);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::DirectoryFrame, offset) == 8);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::DirectoryFrame, child_done) == 12);
    assert!(core::mem::size_of::<super::ntfs_volume_types::DirectoryCursor>() == 136);
    assert!(core::mem::align_of::<super::ntfs_volume_types::DirectoryCursor>() == 8);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::DirectoryCursor, state) == 0);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::DirectoryCursor, depth) == 4);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::DirectoryCursor, frames) == 8);
    assert!(core::mem::size_of::<super::ntfs_volume_types::RecordName>() == 792);
    assert!(core::mem::align_of::<super::ntfs_volume_types::RecordName>() == 8);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::RecordName, parent) == 0);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::RecordName, parent_sequence) == 8);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::RecordName, name) == 10);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::RecordName, name_len) == 784);
    assert!(core::mem::size_of::<super::ntfs_volume_types::ResolvedEntry>() == 840);
    assert!(core::mem::align_of::<super::ntfs_volume_types::ResolvedEntry>() == 8);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::ResolvedEntry, entry) == 0);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::ResolvedEntry, parent_record) == 832);
    assert!(core::mem::size_of::<super::ntfs_volume_types::WriteProgress>() == 24);
    assert!(core::mem::align_of::<super::ntfs_volume_types::WriteProgress>() == 8);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::WriteProgress, status) == 0);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::WriteProgress, completed) == 8);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::WriteProgress, uncertain) == 16);
    assert!(core::mem::size_of::<super::ntfs_volume_types::MountInfo>() == 56);
    assert!(core::mem::align_of::<super::ntfs_volume_types::MountInfo>() == 8);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::MountInfo, cluster_bytes) == 0);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::MountInfo, record_bytes) == 4);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::MountInfo, index_block_bytes) == 8);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::MountInfo, total_sectors) == 16);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::MountInfo, mft_lcn) == 24);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::MountInfo, mftmirr_lcn) == 32);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::MountInfo, mft_run_count) == 40);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::MountInfo, used_mft_mirror) == 48);
    assert!(core::mem::size_of::<super::ntfs_volume_types::MetadataCacheSummary>() == 392);
    assert!(core::mem::align_of::<super::ntfs_volume_types::MetadataCacheSummary>() == 8);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::MetadataCacheSummary, version) == 0);
    assert!(
        core::mem::offset_of!(
            super::ntfs_volume_types::MetadataCacheSummary,
            active_volumes
        ) == 4
    );
    assert!(
        core::mem::offset_of!(
            super::ntfs_volume_types::MetadataCacheSummary,
            slot_capacity
        ) == 8
    );
    assert!(
        core::mem::offset_of!(
            super::ntfs_volume_types::MetadataCacheSummary,
            bytes_per_volume
        ) == 12
    );
    assert!(
        core::mem::offset_of!(
            super::ntfs_volume_types::MetadataCacheSummary,
            record_capacity
        ) == 16
    );
    assert!(
        core::mem::offset_of!(
            super::ntfs_volume_types::MetadataCacheSummary,
            attribute_capacity
        ) == 20
    );
    assert!(
        core::mem::offset_of!(
            super::ntfs_volume_types::MetadataCacheSummary,
            index_capacity
        ) == 24
    );
    assert!(
        core::mem::offset_of!(
            super::ntfs_volume_types::MetadataCacheSummary,
            path_capacity
        ) == 28
    );
    assert!(
        core::mem::offset_of!(
            super::ntfs_volume_types::MetadataCacheSummary,
            mount_generation
        ) == 32
    );
    assert!(
        core::mem::offset_of!(
            super::ntfs_volume_types::MetadataCacheSummary,
            content_generation
        ) == 40
    );
    assert!(
        core::mem::offset_of!(
            super::ntfs_volume_types::MetadataCacheSummary,
            negative_ttl_ticks
        ) == 48
    );
    assert!(
        core::mem::offset_of!(
            super::ntfs_volume_types::MetadataCacheSummary,
            record_entries
        ) == 56
    );
    assert!(
        core::mem::offset_of!(
            super::ntfs_volume_types::MetadataCacheSummary,
            attribute_entries
        ) == 60
    );
    assert!(
        core::mem::offset_of!(
            super::ntfs_volume_types::MetadataCacheSummary,
            index_entries
        ) == 64
    );
    assert!(
        core::mem::offset_of!(super::ntfs_volume_types::MetadataCacheSummary, path_entries) == 68
    );
    assert!(
        core::mem::offset_of!(super::ntfs_volume_types::MetadataCacheSummary, record_hits) == 72
    );
    assert!(
        core::mem::offset_of!(
            super::ntfs_volume_types::MetadataCacheSummary,
            record_misses
        ) == 80
    );
    assert!(
        core::mem::offset_of!(
            super::ntfs_volume_types::MetadataCacheSummary,
            record_stores
        ) == 88
    );
    assert!(
        core::mem::offset_of!(
            super::ntfs_volume_types::MetadataCacheSummary,
            record_evictions
        ) == 96
    );
    assert!(
        core::mem::offset_of!(
            super::ntfs_volume_types::MetadataCacheSummary,
            attribute_hits
        ) == 104
    );
    assert!(
        core::mem::offset_of!(
            super::ntfs_volume_types::MetadataCacheSummary,
            attribute_misses
        ) == 112
    );
    assert!(
        core::mem::offset_of!(
            super::ntfs_volume_types::MetadataCacheSummary,
            attribute_stores
        ) == 120
    );
    assert!(
        core::mem::offset_of!(
            super::ntfs_volume_types::MetadataCacheSummary,
            attribute_evictions
        ) == 128
    );
    assert!(
        core::mem::offset_of!(super::ntfs_volume_types::MetadataCacheSummary, index_hits) == 136
    );
    assert!(
        core::mem::offset_of!(super::ntfs_volume_types::MetadataCacheSummary, index_misses) == 144
    );
    assert!(
        core::mem::offset_of!(super::ntfs_volume_types::MetadataCacheSummary, index_stores) == 152
    );
    assert!(
        core::mem::offset_of!(
            super::ntfs_volume_types::MetadataCacheSummary,
            index_evictions
        ) == 160
    );
    assert!(
        core::mem::offset_of!(super::ntfs_volume_types::MetadataCacheSummary, path_queries) == 168
    );
    assert!(
        core::mem::offset_of!(
            super::ntfs_volume_types::MetadataCacheSummary,
            path_positive_hits
        ) == 176
    );
    assert!(
        core::mem::offset_of!(
            super::ntfs_volume_types::MetadataCacheSummary,
            path_negative_hits
        ) == 184
    );
    assert!(
        core::mem::offset_of!(super::ntfs_volume_types::MetadataCacheSummary, path_misses) == 192
    );
    assert!(
        core::mem::offset_of!(
            super::ntfs_volume_types::MetadataCacheSummary,
            path_positive_stores
        ) == 200
    );
    assert!(
        core::mem::offset_of!(
            super::ntfs_volume_types::MetadataCacheSummary,
            path_negative_stores
        ) == 208
    );
    assert!(
        core::mem::offset_of!(
            super::ntfs_volume_types::MetadataCacheSummary,
            path_expirations
        ) == 216
    );
    assert!(
        core::mem::offset_of!(
            super::ntfs_volume_types::MetadataCacheSummary,
            lookup_tree_walks
        ) == 224
    );
    assert!(
        core::mem::offset_of!(
            super::ntfs_volume_types::MetadataCacheSummary,
            recovery_cache_bypasses
        ) == 232
    );
    assert!(
        core::mem::offset_of!(
            super::ntfs_volume_types::MetadataCacheSummary,
            mount_invalidations
        ) == 240
    );
    assert!(
        core::mem::offset_of!(
            super::ntfs_volume_types::MetadataCacheSummary,
            mutation_invalidations
        ) == 248
    );
    assert!(
        core::mem::offset_of!(
            super::ntfs_volume_types::MetadataCacheSummary,
            external_invalidations
        ) == 256
    );
    assert!(
        core::mem::offset_of!(
            super::ntfs_volume_types::MetadataCacheSummary,
            invalidated_entries
        ) == 264
    );
    assert!(
        core::mem::offset_of!(
            super::ntfs_volume_types::MetadataCacheSummary,
            payload_write_retentions
        ) == 272
    );
    assert!(
        core::mem::offset_of!(
            super::ntfs_volume_types::MetadataCacheSummary,
            system_write_retentions
        ) == 280
    );
    assert!(
        core::mem::offset_of!(
            super::ntfs_volume_types::MetadataCacheSummary,
            targeted_invalidations
        ) == 288
    );
    assert!(
        core::mem::offset_of!(
            super::ntfs_volume_types::MetadataCacheSummary,
            targeted_record_invalidations
        ) == 296
    );
    assert!(
        core::mem::offset_of!(
            super::ntfs_volume_types::MetadataCacheSummary,
            targeted_attribute_invalidations
        ) == 304
    );
    assert!(
        core::mem::offset_of!(
            super::ntfs_volume_types::MetadataCacheSummary,
            targeted_directory_invalidations
        ) == 312
    );
    assert!(
        core::mem::offset_of!(
            super::ntfs_volume_types::MetadataCacheSummary,
            global_mutation_invalidations
        ) == 320
    );
    assert!(
        core::mem::offset_of!(
            super::ntfs_volume_types::MetadataCacheSummary,
            recovery_invalidations
        ) == 328
    );
    assert!(
        core::mem::offset_of!(
            super::ntfs_volume_types::MetadataCacheSummary,
            mutation_invalidated_record_entries
        ) == 336
    );
    assert!(
        core::mem::offset_of!(
            super::ntfs_volume_types::MetadataCacheSummary,
            mutation_invalidated_attribute_entries
        ) == 344
    );
    assert!(
        core::mem::offset_of!(
            super::ntfs_volume_types::MetadataCacheSummary,
            mutation_invalidated_index_entries
        ) == 352
    );
    assert!(
        core::mem::offset_of!(
            super::ntfs_volume_types::MetadataCacheSummary,
            mutation_invalidated_path_entries
        ) == 360
    );
    assert!(
        core::mem::offset_of!(
            super::ntfs_volume_types::MetadataCacheSummary,
            reclaim_requests
        ) == 368
    );
    assert!(
        core::mem::offset_of!(
            super::ntfs_volume_types::MetadataCacheSummary,
            reclaim_scans
        ) == 376
    );
    assert!(
        core::mem::offset_of!(
            super::ntfs_volume_types::MetadataCacheSummary,
            reclaimed_entries
        ) == 384
    );
    assert!(core::mem::size_of::<super::ntfs_volume_types::MetadataReclaimResult>() == 12);
    assert!(core::mem::align_of::<super::ntfs_volume_types::MetadataReclaimResult>() == 4);
    assert!(
        core::mem::offset_of!(
            super::ntfs_volume_types::MetadataReclaimResult,
            requested_entries
        ) == 0
    );
    assert!(
        core::mem::offset_of!(
            super::ntfs_volume_types::MetadataReclaimResult,
            inspected_entries
        ) == 4
    );
    assert!(
        core::mem::offset_of!(
            super::ntfs_volume_types::MetadataReclaimResult,
            reclaimed_entries
        ) == 8
    );
    assert!(core::mem::size_of::<super::ntfs_volume_io::Device>() == 40);
    assert!(core::mem::align_of::<super::ntfs_volume_io::Device>() == 8);
    assert!(core::mem::offset_of!(super::ntfs_volume_io::Device, context) == 0);
    assert!(core::mem::offset_of!(super::ntfs_volume_io::Device, read) == 8);
    assert!(core::mem::offset_of!(super::ntfs_volume_io::Device, write) == 16);
    assert!(core::mem::offset_of!(super::ntfs_volume_io::Device, flush) == 24);
    assert!(core::mem::offset_of!(super::ntfs_volume_io::Device, max_transfer_sectors) == 32);
    assert!(core::mem::size_of::<super::ntfs_volume_io::Io>() == 80);
    assert!(core::mem::align_of::<super::ntfs_volume_io::Io>() == 8);
    assert!(core::mem::offset_of!(super::ntfs_volume_io::Io, device) == 0);
    assert!(core::mem::offset_of!(super::ntfs_volume_io::Io, geometry) == 40);
    assert!(core::mem::offset_of!(super::ntfs_volume_io::Io, scratch) == 56);
    assert!(core::mem::offset_of!(super::ntfs_volume_io::Io, cache) == 64);
    assert!(core::mem::offset_of!(super::ntfs_volume_io::Io, work_counters) == 72);
    assert!(core::mem::size_of::<super::ntfs_volume_types::Volume>() == 152);
    assert!(core::mem::align_of::<super::ntfs_volume_types::Volume>() == 8);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::Volume, io) == 0);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::Volume, record_bytes) == 80);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::Volume, index_block_bytes) == 84);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::Volume, mft_runs) == 88);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::Volume, mft_capacity) == 96);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::Volume, mft_count) == 104);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::Volume, upcase) == 112);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::Volume, now_filetime) == 128);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::Volume, security_id_file) == 136);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::Volume, security_id_dir) == 140);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::Volume, cache_now_ticks) == 144);
    assert!(core::mem::size_of::<super::ntfs_volume_entry::Args>() == 120);
    assert!(core::mem::align_of::<super::ntfs_volume_entry::Args>() == 8);
    assert!(core::mem::offset_of!(super::ntfs_volume_entry::Args, volume) == 0);
    assert!(core::mem::offset_of!(super::ntfs_volume_entry::Args, state) == 8);
    assert!(core::mem::offset_of!(super::ntfs_volume_entry::Args, out) == 16);
    assert!(core::mem::offset_of!(super::ntfs_volume_entry::Args, buffer) == 24);
    assert!(core::mem::offset_of!(super::ntfs_volume_entry::Args, name) == 40);
    assert!(core::mem::offset_of!(super::ntfs_volume_entry::Args, other) == 56);
    assert!(core::mem::offset_of!(super::ntfs_volume_entry::Args, third) == 72);
    assert!(core::mem::offset_of!(super::ntfs_volume_entry::Args, number) == 88);
    assert!(core::mem::offset_of!(super::ntfs_volume_entry::Args, second) == 96);
    assert!(core::mem::offset_of!(super::ntfs_volume_entry::Args, third_number) == 104);
    assert!(core::mem::offset_of!(super::ntfs_volume_entry::Args, kind) == 112);
    assert!(core::mem::offset_of!(super::ntfs_volume_entry::Args, flag) == 116);
    assert!(core::mem::size_of::<super::ntfs_volume_entry::EnumSink>() == 928);
    assert!(core::mem::align_of::<super::ntfs_volume_entry::EnumSink>() == 8);
    assert!(core::mem::offset_of!(super::ntfs_volume_entry::EnumSink, out) == 0);
    assert!(core::mem::offset_of!(super::ntfs_volume_entry::EnumSink, has_out) == 16);
    assert!(core::mem::offset_of!(super::ntfs_volume_entry::EnumSink, cursor) == 24);
    assert!(core::mem::offset_of!(super::ntfs_volume_entry::EnumSink, listed) == 32);
    assert!(core::mem::offset_of!(super::ntfs_volume_entry::EnumSink, max_entries) == 40);
    assert!(core::mem::offset_of!(super::ntfs_volume_entry::EnumSink, wanted) == 48);
    assert!(core::mem::offset_of!(super::ntfs_volume_entry::EnumSink, seen) == 56);
    assert!(core::mem::offset_of!(super::ntfs_volume_entry::EnumSink, found) == 64);
    assert!(core::mem::offset_of!(super::ntfs_volume_entry::EnumSink, has_found) == 896);
    assert!(core::mem::offset_of!(super::ntfs_volume_entry::EnumSink, failed) == 897);
    assert!(core::mem::offset_of!(super::ntfs_volume_entry::EnumSink, count_ref) == 904);
    assert!(core::mem::offset_of!(super::ntfs_volume_entry::EnumSink, has_count_ref) == 912);
    assert!(core::mem::offset_of!(super::ntfs_volume_entry::EnumSink, matches) == 920);
    assert!(core::mem::size_of::<super::ntfs_adapter_types::Mounted>() == 16);
    assert!(core::mem::align_of::<super::ntfs_adapter_types::Mounted>() == 8);
    assert!(core::mem::offset_of!(super::ntfs_adapter_types::Mounted, state_slot) == 0);
    assert!(core::mem::offset_of!(super::ntfs_adapter_types::Mounted, cluster_bytes) == 4);
    assert!(core::mem::offset_of!(super::ntfs_adapter_types::Mounted, total_sectors) == 8);
    assert!(core::mem::size_of::<super::ntfs_adapter_types::Entry>() == 824);
    assert!(core::mem::align_of::<super::ntfs_adapter_types::Entry>() == 8);
    assert!(core::mem::offset_of!(super::ntfs_adapter_types::Entry, name) == 0);
    assert!(core::mem::offset_of!(super::ntfs_adapter_types::Entry, name_len) == 768);
    assert!(core::mem::offset_of!(super::ntfs_adapter_types::Entry, attr) == 776);
    assert!(core::mem::offset_of!(super::ntfs_adapter_types::Entry, reparse) == 777);
    assert!(core::mem::offset_of!(super::ntfs_adapter_types::Entry, record) == 784);
    assert!(core::mem::offset_of!(super::ntfs_adapter_types::Entry, sequence) == 792);
    assert!(core::mem::offset_of!(super::ntfs_adapter_types::Entry, size) == 800);
    assert!(core::mem::offset_of!(super::ntfs_adapter_types::Entry, created_time) == 808);
    assert!(core::mem::offset_of!(super::ntfs_adapter_types::Entry, created_date) == 810);
    assert!(core::mem::offset_of!(super::ntfs_adapter_types::Entry, access_date) == 812);
    assert!(core::mem::offset_of!(super::ntfs_adapter_types::Entry, modified_time) == 814);
    assert!(core::mem::offset_of!(super::ntfs_adapter_types::Entry, modified_date) == 816);
    assert!(core::mem::size_of::<super::ntfs_adapter_types::Args>() == 136);
    assert!(core::mem::align_of::<super::ntfs_adapter_types::Args>() == 8);
    assert!(core::mem::offset_of!(super::ntfs_adapter_types::Args, volume) == 0);
    assert!(core::mem::offset_of!(super::ntfs_adapter_types::Args, entry) == 16);
    assert!(core::mem::offset_of!(super::ntfs_adapter_types::Args, buffer) == 24);
    assert!(core::mem::offset_of!(super::ntfs_adapter_types::Args, name) == 40);
    assert!(core::mem::offset_of!(super::ntfs_adapter_types::Args, other) == 56);
    assert!(core::mem::offset_of!(super::ntfs_adapter_types::Args, third) == 72);
    assert!(core::mem::offset_of!(super::ntfs_adapter_types::Args, number) == 88);
    assert!(core::mem::offset_of!(super::ntfs_adapter_types::Args, second) == 96);
    assert!(core::mem::offset_of!(super::ntfs_adapter_types::Args, count) == 104);
    assert!(core::mem::offset_of!(super::ntfs_adapter_types::Args, device) == 112);
    assert!(core::mem::offset_of!(super::ntfs_adapter_types::Args, state) == 120);
    assert!(core::mem::offset_of!(super::ntfs_adapter_types::Args, flag) == 128);
    assert!(core::mem::size_of::<super::ntfs_volume_types::Scratch>() == 213168);
    assert!(core::mem::size_of::<super::ntfs_volume_cache::Cache>() == 84328);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::Scratch, work) == 0);
    assert!(core::mem::offset_of!(super::ntfs_volume_types::Scratch, record) == 11376);
};
