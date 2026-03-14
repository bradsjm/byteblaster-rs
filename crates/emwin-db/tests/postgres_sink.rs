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
    CompletedFileMetadata::build(
        "FFWOAXNE.TXT",
        1_704_070_800,
        ProductOrigin::Qbt,
        br#"000
WUUS53 KOAX 051200
FFWOAX

Flash Flood Warning
National Weather Service Omaha/Valley NE
1200 PM CST Wed Mar 5 2025

NEC001>003-051300-
/O.NEW.KOAX.FF.W.0001.250305T1200Z-250305T1800Z/
/MSRM1.3.ER.250305T1200Z.250305T1800Z.250306T0000Z.NO/

LAT...LON 4143 9613 4145 9610 4140 9608 4138 9612
TIME...MOT...LOC 1200Z 300DEG 25KT 4143 9613 4140 9608
MAXHAILSIZE...1.00 IN
MAXWINDGUST...60 MPH
"#,
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
        "SELECT id, source_receiver, source_message_id, ingested_at, payload_location, metadata_location, has_vtec, has_ugc, has_hvtec, has_latlon, has_time_mot_loc, has_wind_hail \
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
}
