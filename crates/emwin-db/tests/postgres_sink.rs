use emwin_db::{
    BlobRole, BlobStorageKind, CompletedFileMetadata, MetadataSink, PersistedRequest,
    PostgresConfig, PostgresMetadataSink, StoredBlob,
};
use emwin_protocol::ingest::ProductOrigin;
use sqlx::Row;

fn test_database_url() -> Option<String> {
    std::env::var("EMWIN_PG_TEST_DATABASE_URL").ok()
}

fn sample_metadata() -> CompletedFileMetadata {
    sample_metadata_with_action(1_704_070_800, "NEW", 101)
}

fn sample_metadata_with_action(
    timestamp_utc: u64,
    action: &str,
    etn: u32,
) -> CompletedFileMetadata {
    CompletedFileMetadata::build(
        "FFWOAXNE.TXT",
        timestamp_utc,
        ProductOrigin::Qbt,
        format!(
            "000
WUUS53 KOAX 051200
FFWOAX

Flash Flood Warning
National Weather Service Omaha/Valley NE
1200 PM CST Wed Mar 5 2025

NEC001>003-051300-
            /O.{action}.KOAX.FF.W.{etn:04}.250305T1200Z-250305T1800Z/
/MSRM1.3.ER.250305T1200Z.250305T1800Z.250306T0000Z.NO/

LAT...LON 4143 9613 4145 9610 4140 9608 4138 9612
TIME...MOT...LOC 1200Z 300DEG 25KT 4143 9613 4140 9608
MAXHAILSIZE...1.00 IN
MAXWINDGUST...60 MPH
"
        )
        .as_bytes(),
    )
}

fn sample_blobs() -> Vec<StoredBlob> {
    vec![
        StoredBlob {
            kind: BlobStorageKind::Filesystem,
            role: BlobRole::Payload,
            location: "/tmp/FFWOAXNE.TXT".to_string(),
            size_bytes: 512,
            content_type: Some("application/octet-stream".to_string()),
        },
        StoredBlob {
            kind: BlobStorageKind::Filesystem,
            role: BlobRole::MetadataSidecar,
            location: "/tmp/FFWOAXNE.JSON".to_string(),
            size_bytes: 256,
            content_type: Some("application/json".to_string()),
        },
    ]
}

#[tokio::test]
async fn postgres_sink_bootstraps_and_persists_rows() {
    let Some(database_url) = test_database_url() else {
        return;
    };

    let mut config = PostgresConfig::new(database_url);
    config.application_name = "emwin-db-test".to_string();
    let sink = PostgresMetadataSink::connect(config)
        .await
        .expect("postgres sink should connect");

    let metadata = sample_metadata();
    sink.persist(PersistedRequest {
        request_key: metadata.filename.clone(),
        metadata: metadata.clone(),
        blobs: sample_blobs(),
    })
    .await
    .expect("postgres sink should persist metadata");

    let row = sqlx::query(
        "SELECT id, source_receiver, source_message_id, ingested_at, payload_location, metadata_location, has_vtec, has_ugc, has_hvtec, has_latlon, has_time_mot_loc, has_wind_hail, product_json \
         FROM products WHERE filename = $1 AND source_timestamp_utc = $2",
    )
    .bind(&metadata.filename)
    .bind(i64::try_from(metadata.timestamp_utc).expect("timestamp should fit in bigint"))
    .fetch_one(sink.pool())
    .await
    .expect("persisted product row should exist");

    let product_id = row.get::<i64, _>("id");
    assert_eq!(row.get::<String, _>("source_receiver"), "qbt");
    assert_eq!(row.get::<Option<String>, _>("source_message_id"), None);
    assert!(row.get::<chrono::DateTime<chrono::Utc>, _>("ingested_at") <= chrono::Utc::now());
    assert_eq!(
        row.get::<String, _>("payload_location"),
        "/tmp/FFWOAXNE.TXT"
    );
    assert_eq!(
        row.get::<Option<String>, _>("metadata_location").as_deref(),
        Some("/tmp/FFWOAXNE.JSON")
    );
    assert!(row.get::<bool, _>("has_vtec"));
    assert!(row.get::<bool, _>("has_ugc"));
    assert!(row.get::<bool, _>("has_hvtec"));
    assert!(row.get::<bool, _>("has_latlon"));
    assert!(row.get::<bool, _>("has_time_mot_loc"));
    assert!(row.get::<bool, _>("has_wind_hail"));
    let product_json = row.get::<serde_json::Value, _>("product_json");
    assert_eq!(product_json["filename"], metadata.filename);
    assert!(product_json["product"].is_null());
    assert_eq!(product_json["incidents"][0]["office"], "KOAX");

    let origin_json_column_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM information_schema.columns WHERE table_name = 'products' AND column_name = 'origin_json'",
    )
    .fetch_one(sink.pool())
    .await
    .expect("products schema should be queryable");
    assert_eq!(origin_json_column_count, 0);

    let summary_json_column_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM information_schema.columns WHERE table_name = 'products' AND column_name = 'summary_json'",
    )
    .fetch_one(sink.pool())
    .await
    .expect("products schema should be queryable");
    assert_eq!(summary_json_column_count, 0);

    let pruned_summary_column_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM information_schema.columns WHERE table_name = 'products' AND column_name = ANY($1)",
    )
    .bind(vec![
        "issue_codes",
        "vtec_phenomena",
        "vtec_significance",
        "vtec_actions",
        "vtec_offices",
        "etns",
        "hvtec_nwslids",
        "hvtec_causes",
        "hvtec_severities",
        "hvtec_records",
    ])
    .fetch_one(sink.pool())
    .await
    .expect("products schema should be queryable");
    assert_eq!(pruned_summary_column_count, 0);

    let vtec_count =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM product_vtec WHERE product_id = $1")
            .bind(product_id)
            .fetch_one(sink.pool())
            .await
            .expect("vtec rows should be queryable");
    let ugc_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM product_ugc_areas WHERE product_id = $1",
    )
    .bind(product_id)
    .fetch_one(sink.pool())
    .await
    .expect("ugc rows should be queryable");
    let hvtec_count =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM product_hvtec WHERE product_id = $1")
            .bind(product_id)
            .fetch_one(sink.pool())
            .await
            .expect("hvtec rows should be queryable");
    let polygon_count =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM product_polygons WHERE product_id = $1")
            .bind(product_id)
            .fetch_one(sink.pool())
            .await
            .expect("polygon rows should be queryable");
    let time_mot_loc_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM product_time_mot_loc WHERE product_id = $1",
    )
    .bind(product_id)
    .fetch_one(sink.pool())
    .await
    .expect("time mot loc rows should be queryable");
    let wind_hail_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM product_wind_hail WHERE product_id = $1",
    )
    .bind(product_id)
    .fetch_one(sink.pool())
    .await
    .expect("wind hail rows should be queryable");

    assert_eq!(vtec_count, 1);
    assert_eq!(ugc_count, 3);
    assert_eq!(hvtec_count, 1);
    assert_eq!(polygon_count, 1);
    assert_eq!(time_mot_loc_count, 1);
    assert_eq!(wind_hail_count, 2);

    let incident_row = sqlx::query(
        "SELECT id, source, source_timestamp_utc, product_json FROM products WHERE filename = $1 AND source_timestamp_utc = 0",
    )
    .bind("__incident__/KOAX/FF/W/101")
    .fetch_one(sink.pool())
    .await
    .expect("incident row should exist");
    let incident_id = incident_row.get::<i64, _>("id");
    assert_eq!(incident_row.get::<String, _>("source"), "incident");
    assert_eq!(incident_row.get::<i64, _>("source_timestamp_utc"), 0);
    let incident_json = incident_row.get::<serde_json::Value, _>("product_json");
    assert_eq!(incident_json["office"], "KOAX");
    assert_eq!(incident_json["phenomena"], "FF");
    assert_eq!(incident_json["significance"], "W");
    assert_eq!(incident_json["etn"], 101);
    assert_eq!(incident_json["current_status"], "active");
    assert_eq!(incident_json["latest_vtec_action"], "NEW");
    assert_eq!(incident_json["first_product_id"], product_id);
    assert_eq!(incident_json["latest_product_id"], product_id);
    assert_eq!(
        incident_json["latest_product_timestamp_utc"],
        1_704_070_800_i64
    );

    let incident_vtec = sqlx::query(
        "SELECT status, action, office, phenomena, significance, etn FROM product_vtec WHERE product_id = $1",
    )
    .bind(incident_id)
    .fetch_one(sink.pool())
    .await
    .expect("incident vtec row should exist");
    assert_eq!(incident_vtec.get::<String, _>("status"), "O");
    assert_eq!(incident_vtec.get::<String, _>("action"), "NEW");
    assert_eq!(incident_vtec.get::<String, _>("office"), "KOAX");
    assert_eq!(incident_vtec.get::<String, _>("phenomena"), "FF");
    assert_eq!(incident_vtec.get::<String, _>("significance"), "W");
    assert_eq!(incident_vtec.get::<i32, _>("etn"), 101);
}

#[tokio::test]
async fn postgres_sink_updates_incident_rows_and_rejects_stale_updates() {
    let Some(database_url) = test_database_url() else {
        return;
    };

    let mut config = PostgresConfig::new(database_url);
    config.application_name = "emwin-db-test".to_string();
    let sink = PostgresMetadataSink::connect(config)
        .await
        .expect("postgres sink should connect");

    let initial = sample_metadata_with_action(1_704_070_800, "NEW", 202);
    sink.persist(PersistedRequest {
        request_key: format!("{}-{}", initial.filename, initial.timestamp_utc),
        metadata: initial.clone(),
        blobs: sample_blobs(),
    })
    .await
    .expect("initial metadata should persist");

    let updated = sample_metadata_with_action(1_704_071_100, "CAN", 202);
    sink.persist(PersistedRequest {
        request_key: format!("{}-{}", updated.filename, updated.timestamp_utc),
        metadata: updated.clone(),
        blobs: sample_blobs(),
    })
    .await
    .expect("updated metadata should persist");

    let incident_after_update = sqlx::query(
        "SELECT product_json FROM products WHERE filename = $1 AND source_timestamp_utc = 0",
    )
    .bind("__incident__/KOAX/FF/W/202")
    .fetch_one(sink.pool())
    .await
    .expect("incident row should exist after update")
    .get::<serde_json::Value, _>("product_json");
    assert_eq!(incident_after_update["current_status"], "cancelled");
    assert_eq!(incident_after_update["latest_vtec_action"], "CAN");
    assert_eq!(
        incident_after_update["latest_product_timestamp_utc"],
        1_704_071_100_i64
    );
    let first_product_id = incident_after_update["first_product_id"]
        .as_i64()
        .expect("first product id should be present");
    let latest_product_id = incident_after_update["latest_product_id"]
        .as_i64()
        .expect("latest product id should be present");
    assert_ne!(first_product_id, latest_product_id);

    let stale = sample_metadata_with_action(1_704_070_000, "EXP", 202);
    sink.persist(PersistedRequest {
        request_key: format!("{}-{}", stale.filename, stale.timestamp_utc),
        metadata: stale,
        blobs: sample_blobs(),
    })
    .await
    .expect("stale source product should still persist");

    let incident_after_stale = sqlx::query(
        "SELECT product_json FROM products WHERE filename = $1 AND source_timestamp_utc = 0",
    )
    .bind("__incident__/KOAX/FF/W/202")
    .fetch_one(sink.pool())
    .await
    .expect("incident row should still exist after stale update")
    .get::<serde_json::Value, _>("product_json");
    assert_eq!(incident_after_stale["current_status"], "cancelled");
    assert_eq!(incident_after_stale["latest_vtec_action"], "CAN");
    assert_eq!(
        incident_after_stale["latest_product_timestamp_utc"],
        1_704_071_100_i64
    );
}
