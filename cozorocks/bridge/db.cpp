// Copyright 2022, The Cozo Project Authors.
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

#include <iostream>
#include <memory>
#include "db.h"
#include "cozorocks/src/bridge/mod.rs.h"
#include "rocksdb/utilities/options_util.h"

Options default_db_options()
{
    Options options = Options();
    options.bottommost_compression = kZSTD;
    options.compression = kLZ4Compression;
    options.level_compaction_dynamic_level_bytes = true;
    options.max_background_jobs = 6;
    options.bytes_per_sync = 1048576;
    options.compaction_pri = kMinOverlappingRatio;
    BlockBasedTableOptions table_options;
    table_options.block_size = 16 * 1024;
    table_options.cache_index_and_filter_blocks = true;
    table_options.pin_l0_filter_and_index_blocks_in_cache = true;
    table_options.format_version = 5;

    auto table_factory = NewBlockBasedTableFactory(table_options);
    options.table_factory.reset(table_factory);

    return options;
}

ColumnFamilyOptions default_cf_options()
{
    ColumnFamilyOptions options = ColumnFamilyOptions();
    options.bottommost_compression = kZSTD;
    options.compression = kLZ4Compression;
    options.level_compaction_dynamic_level_bytes = true;
    options.compaction_pri = kMinOverlappingRatio;

    BlockBasedTableOptions table_options;

    table_options.block_size = 16 * 1024;
    table_options.cache_index_and_filter_blocks = true;
    table_options.pin_l0_filter_and_index_blocks_in_cache = true;
    table_options.format_version = 5;

    auto table_factory = NewBlockBasedTableFactory(table_options);
    options.table_factory.reset(table_factory);

    return options;
}

shared_ptr<RocksDbBridge> open_db(const DbOpts &opts, RocksDbStatus &status)
{
    auto options = default_db_options();

    // Block cache: size now comes from DbOpts (it used to be hard-coded to 1 GiB
    // and was never actually applied outside of the options-file branch).
    shared_ptr<Cache> cache = nullptr;

    if (opts.block_cache_size > 0)
    {
        cache = NewLRUCache(opts.block_cache_size);
    }

    if (!opts.options_path.empty())
    {
        DBOptions loaded_db_opt;
        std::vector<ColumnFamilyDescriptor> loaded_cf_descs;
        ConfigOptions config_options;
        string options_path = convert_vec_to_string(opts.options_path);
        Status s = LoadOptionsFromFile(config_options, options_path, &loaded_db_opt,
                                       &loaded_cf_descs);
        if (!s.ok())
        {
            write_status(s, status);
            return nullptr;
        }

        if (loaded_cf_descs.empty())
        {
            write_status(Status::InvalidArgument("options file contains no column family"), status);
            return nullptr;
        }

        // Fix: only the first (default) column family is used below, and the
        // previous loop indexed [0] on every iteration.
        if (cache != nullptr)
        {
            auto *loaded_bbt_opt =
                loaded_cf_descs[0]
                    .options.table_factory->GetOptions<BlockBasedTableOptions>();
            if (loaded_bbt_opt != nullptr)
            {
                loaded_bbt_opt->block_cache = cache;
            }
        }

        options = Options(loaded_db_opt, loaded_cf_descs[0].options);
    }

    if (opts.prepare_for_bulk_load)
    {
        options.PrepareForBulkLoad();
    }
    if (opts.increase_parallelism > 0)
    {
        options.IncreaseParallelism(opts.increase_parallelism);
    }
    if (opts.optimize_level_style_compaction)
    {
        options.OptimizeLevelStyleCompaction();
    }
    options.create_if_missing = opts.create_if_missing;
    options.paranoid_checks = opts.paranoid_checks;
    if (opts.enable_blob_files)
    {
        options.enable_blob_files = true;

        options.min_blob_size = opts.min_blob_size;

        options.blob_file_size = opts.blob_file_size;

        options.enable_blob_garbage_collection = opts.enable_blob_garbage_collection;
    }

    // Fix: start from the table options already in place (block_size, index/filter
    // pinning, format_version, or whatever the options file defined) instead of
    // replacing them with a brand new default BlockBasedTableOptions.
    if (opts.use_bloom_filter || cache != nullptr)
    {
        BlockBasedTableOptions table_options;
        if (options.table_factory != nullptr)
        {
            auto *existing = options.table_factory->GetOptions<BlockBasedTableOptions>();
            if (existing != nullptr)
            {
                table_options = *existing;
            }
        }
        if (opts.use_bloom_filter)
        {
            table_options.filter_policy.reset(
                NewBloomFilterPolicy(opts.bloom_filter_bits_per_key, false));
            table_options.whole_key_filtering = opts.bloom_filter_whole_key_filtering;
        }
        if (cache != nullptr)
        {
            table_options.block_cache = cache;
        }
        options.table_factory.reset(NewBlockBasedTableFactory(table_options));
    }

    if (opts.use_capped_prefix_extractor)
    {
        options.prefix_extractor.reset(NewCappedPrefixTransform(opts.capped_prefix_extractor_len));
    }
    if (opts.use_fixed_prefix_extractor)
    {
        options.prefix_extractor.reset(NewFixedPrefixTransform(opts.fixed_prefix_extractor_len));
    }
    options.create_missing_column_families = true;

    shared_ptr<RocksDbBridge> db = make_shared<RocksDbBridge>();

    db->db_path = convert_vec_to_string(opts.db_path);

    TransactionDB *txn_db = nullptr;
    write_status(
        TransactionDB::Open(options, TransactionDBOptions(), db->db_path, &txn_db),
        status);
    db->db.reset(txn_db);
    db->destroy_on_exit = opts.destroy_on_exit;

    return db;
}

RocksDbBridge::~RocksDbBridge()
{
    if (destroy_on_exit && (db != nullptr))
    {
        cerr << "destroying database on exit: " << db_path << endl;
        auto status = db->Close();
        if (!status.ok())
        {
            cerr << status.ToString() << endl;
        }
        db.reset();
        Options options{};
        auto status2 = DestroyDB(db_path, options);
        if (!status2.ok())
        {
            cerr << status2.ToString() << endl;
        }
    }
}