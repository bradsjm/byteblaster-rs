use crate::error::{PersistError, PersistResult};
use crate::metadata::CompletedFileMetadata;
use crate::runtime::{MetadataSink, PersistedRequest};
use crate::writer::{BlobRole, BlobStorageKind, BoxFuture, StoredBlob};
use emwin_parser::{
    GenericBody, HvtecCode, ProductBody, ProductHeaderV2, TimeMotLocEntry, UgcArea, UgcSection,
    VtecCode, VtecEventBody,
};
use emwin_protocol::ingest::ProductOrigin;
use serde::Serialize;
use serde_json::Value;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{PgPool, Postgres, QueryBuilder, Row, Transaction};
use std::str::FromStr;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Connection settings for the Postgres/PostGIS metadata sink.
#[derive(Debug, Clone)]
pub struct PostgresConfig {
    /// Postgres connection URL.
    pub database_url: String,
    /// Application name reported to Postgres for observability.
    pub application_name: String,
    /// Maximum pool size. Default remains `1` to preserve queue ordering.
    pub max_connections: u32,
}

impl PostgresConfig {
    /// Creates a config with conservative defaults for the single-worker runtime.
    pub fn new(database_url: impl Into<String>) -> Self {
        Self {
            database_url: database_url.into(),
            application_name: "emwin-db".to_string(),
            max_connections: 1,
        }
    }
}

/// Postgres metadata sink backed by an auto-migrated PostGIS schema.
#[derive(Debug, Clone)]
pub struct PostgresMetadataSink {
    pool: PgPool,
}

impl PostgresMetadataSink {
    /// Connects, validates PostGIS availability, and applies embedded migrations.
    pub async fn connect(config: PostgresConfig) -> PersistResult<Self> {
        if config.database_url.trim().is_empty() {
            return Err(PersistError::InvalidConfig(
                "postgres database url must not be empty".to_string(),
            ));
        }
        if config.application_name.trim().is_empty() {
            return Err(PersistError::InvalidConfig(
                "postgres application name must not be empty".to_string(),
            ));
        }

        let options = PgConnectOptions::from_str(&config.database_url)?
            .application_name(&config.application_name);
        let pool = PgPoolOptions::new()
            .max_connections(config.max_connections.max(1))
            .connect_with(options)
            .await?;

        MIGRATOR.run(&pool).await?;
        sqlx::query_scalar::<_, String>("SELECT postgis_version()")
            .fetch_one(&pool)
            .await?;

        Ok(Self { pool })
    }

    /// Exposes the underlying pool for integration tests and diagnostics.
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

impl MetadataSink<CompletedFileMetadata> for PostgresMetadataSink {
    fn persist<'a>(
        &'a self,
        request: PersistedRequest<CompletedFileMetadata>,
    ) -> BoxFuture<'a, PersistResult<()>> {
        Box::pin(async move {
            let prepared = PreparedProduct::prepare(&request.metadata, &request.blobs)?;
            let mut tx = self.pool.begin().await?;
            let product_id = upsert_product(&mut tx, &prepared).await?;
            replace_children(&mut tx, product_id, &prepared).await?;
            tx.commit().await?;
            Ok(())
        })
    }
}

#[derive(Debug)]
struct PreparedProduct {
    row: ProductRow,
    issues: Vec<ProductIssueRow>,
    vtec: Vec<ProductVtecRow>,
    ugc_areas: Vec<ProductUgcAreaRow>,
    hvtec: Vec<ProductHvtecRow>,
    time_mot_loc: Vec<ProductTimeMotLocRow>,
    polygons: Vec<ProductPolygonRow>,
    wind_hail: Vec<ProductWindHailRow>,
    search_points: Vec<ProductSearchPointRow>,
}

#[derive(Debug)]
struct ProductRow {
    filename: String,
    source_timestamp_utc: i64,
    source_receiver: String,
    source_message_id: Option<String>,
    size_bytes: i64,
    payload_storage_kind: String,
    payload_location: String,
    metadata_storage_kind: Option<String>,
    metadata_location: Option<String>,
    source: String,
    family: Option<String>,
    artifact_kind: Option<String>,
    title: Option<String>,
    container: String,
    pil: Option<String>,
    wmo_prefix: Option<String>,
    bbb_kind: Option<String>,
    office_code: Option<String>,
    office_city: Option<String>,
    office_state: Option<String>,
    header_kind: Option<String>,
    ttaaii: Option<String>,
    cccc: Option<String>,
    ddhhmm: Option<String>,
    bbb: Option<String>,
    afos: Option<String>,
    has_body: bool,
    has_artifact: bool,
    has_issues: bool,
    has_vtec: bool,
    has_ugc: bool,
    has_hvtec: bool,
    has_latlon: bool,
    has_time_mot_loc: bool,
    has_wind_hail: bool,
    vtec_count: i32,
    ugc_count: i32,
    hvtec_count: i32,
    latlon_count: i32,
    time_mot_loc_count: i32,
    wind_hail_count: i32,
    issue_count: i32,
    states: Vec<String>,
    ugc_codes: Vec<String>,
    product_json: Value,
}

#[derive(Debug)]
struct ProductIssueRow {
    kind: String,
    code: String,
    message: String,
    line: Option<String>,
}

#[derive(Debug)]
struct ProductVtecRow {
    segment_index: Option<i32>,
    status: String,
    action: String,
    office: String,
    phenomena: String,
    significance: String,
    etn: i64,
    begin_utc: Option<chrono::DateTime<chrono::Utc>>,
    end_utc: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug)]
struct ProductUgcAreaRow {
    segment_index: Option<i32>,
    section_index: i32,
    area_kind: String,
    state: String,
    ugc_code: String,
    name: Option<String>,
    expires_utc: chrono::DateTime<chrono::Utc>,
    latitude: Option<f64>,
    longitude: Option<f64>,
}

#[derive(Debug)]
struct ProductHvtecRow {
    segment_index: Option<i32>,
    hvtec_index: i32,
    nwslid: String,
    location_name: Option<String>,
    severity: String,
    cause: String,
    record: String,
    begin_utc: Option<chrono::DateTime<chrono::Utc>>,
    crest_utc: Option<chrono::DateTime<chrono::Utc>>,
    end_utc: Option<chrono::DateTime<chrono::Utc>>,
    latitude: Option<f64>,
    longitude: Option<f64>,
}

#[derive(Debug)]
struct ProductTimeMotLocRow {
    segment_index: Option<i32>,
    entry_index: i32,
    time_utc: chrono::DateTime<chrono::Utc>,
    direction_degrees: i32,
    speed_kt: i32,
    path_wkt: String,
}

#[derive(Debug)]
struct ProductPolygonRow {
    segment_index: Option<i32>,
    polygon_index: i32,
    polygon_wkt: String,
}

#[derive(Debug)]
struct ProductWindHailRow {
    segment_index: Option<i32>,
    entry_index: i32,
    kind: String,
    numeric_value: Option<f64>,
    units: Option<String>,
    comparison: Option<String>,
}

#[derive(Debug)]
struct ProductSearchPointRow {
    source_kind: String,
    source_index: i32,
    latitude: f64,
    longitude: f64,
}

#[derive(Debug)]
struct UgcBucketSpec<'a> {
    area_kind: &'a str,
    bucket: &'a std::collections::BTreeMap<String, Vec<UgcArea>>,
    code_prefix: char,
}

#[derive(Debug)]
struct HeaderColumns {
    header_kind: Option<String>,
    ttaaii: Option<String>,
    cccc: Option<String>,
    ddhhmm: Option<String>,
    bbb: Option<String>,
    afos: Option<String>,
}

impl PreparedProduct {
    fn prepare(metadata: &CompletedFileMetadata, blobs: &[StoredBlob]) -> PersistResult<Self> {
        let payload = find_blob(blobs, BlobRole::Payload)?;
        let sidecar = find_blob_optional(blobs, BlobRole::MetadataSidecar);
        let header = metadata.product_summary.header.as_ref();
        let HeaderColumns {
            header_kind,
            ttaaii,
            cccc,
            ddhhmm,
            bbb,
            afos,
        } = flatten_header(header);

        let row = ProductRow {
            filename: metadata.filename.clone(),
            source_timestamp_utc: i64::try_from(metadata.timestamp_utc).map_err(|_| {
                PersistError::InvalidRequest(format!(
                    "timestamp `{}` does not fit in bigint",
                    metadata.timestamp_utc
                ))
            })?,
            source_receiver: source_receiver(&metadata.origin).to_string(),
            source_message_id: source_message_id(&metadata.origin),
            size_bytes: i64::try_from(metadata.size).map_err(|_| {
                PersistError::InvalidRequest(format!(
                    "size `{}` does not fit in bigint",
                    metadata.size
                ))
            })?,
            payload_storage_kind: blob_storage_kind(payload.kind).to_string(),
            payload_location: payload.location.clone(),
            metadata_storage_kind: sidecar.map(|blob| blob_storage_kind(blob.kind).to_string()),
            metadata_location: sidecar.map(|blob| blob.location.clone()),
            source: serde_label(&metadata.product_summary.source)?,
            family: metadata.product_summary.family.map(str::to_string),
            artifact_kind: metadata.product_summary.artifact_kind.map(str::to_string),
            title: metadata.product_summary.title.map(str::to_string),
            container: metadata.product_summary.container.to_string(),
            pil: metadata.product_summary.pil.clone(),
            wmo_prefix: metadata.product_summary.wmo_prefix.map(str::to_string),
            bbb_kind: metadata
                .product_summary
                .bbb_kind
                .as_ref()
                .map(serde_label)
                .transpose()?,
            office_code: metadata
                .product_summary
                .office
                .as_ref()
                .map(|office| office.code.to_string()),
            office_city: metadata
                .product_summary
                .office
                .as_ref()
                .map(|office| office.city.to_string()),
            office_state: metadata
                .product_summary
                .office
                .as_ref()
                .map(|office| office.state.to_string()),
            header_kind,
            ttaaii,
            cccc,
            ddhhmm,
            bbb,
            afos,
            has_body: metadata.product_summary.facets.has_body,
            has_artifact: metadata.product_summary.facets.has_artifact,
            has_issues: metadata.product_summary.facets.has_issues,
            has_vtec: metadata.product_summary.facets.vtec_count > 0,
            has_ugc: metadata.product_summary.facets.ugc_count > 0,
            has_hvtec: metadata.product_summary.facets.hvtec_count > 0,
            has_latlon: metadata.product_summary.facets.latlon_count > 0,
            has_time_mot_loc: metadata.product_summary.facets.time_mot_loc_count > 0,
            has_wind_hail: metadata.product_summary.facets.wind_hail_count > 0,
            vtec_count: usize_to_i32(metadata.product_summary.facets.vtec_count, "vtec_count")?,
            ugc_count: usize_to_i32(metadata.product_summary.facets.ugc_count, "ugc_count")?,
            hvtec_count: usize_to_i32(metadata.product_summary.facets.hvtec_count, "hvtec_count")?,
            latlon_count: usize_to_i32(
                metadata.product_summary.facets.latlon_count,
                "latlon_count",
            )?,
            time_mot_loc_count: usize_to_i32(
                metadata.product_summary.facets.time_mot_loc_count,
                "time_mot_loc_count",
            )?,
            wind_hail_count: usize_to_i32(
                metadata.product_summary.facets.wind_hail_count,
                "wind_hail_count",
            )?,
            issue_count: usize_to_i32(metadata.product_summary.issues.count, "issue_count")?,
            states: metadata.product_summary.keys.states.clone(),
            ugc_codes: metadata.product_summary.keys.ugc_codes.clone(),
            product_json: serde_json::to_value(&metadata.product_detail)?,
        };

        let issues = metadata
            .product
            .issues
            .iter()
            .map(|issue| ProductIssueRow {
                kind: issue.kind.to_string(),
                code: issue.code.to_string(),
                message: issue.message.clone(),
                line: issue.line.clone(),
            })
            .collect();

        let mut prepared = Self {
            row,
            issues,
            vtec: Vec::new(),
            ugc_areas: Vec::new(),
            hvtec: Vec::new(),
            time_mot_loc: Vec::new(),
            polygons: Vec::new(),
            wind_hail: Vec::new(),
            search_points: Vec::new(),
        };

        if let Some(body) = metadata.product.body.as_ref() {
            collect_body_rows(&mut prepared, body)?;
        }

        Ok(prepared)
    }
}

fn collect_body_rows(prepared: &mut PreparedProduct, body: &ProductBody) -> PersistResult<()> {
    match body {
        ProductBody::VtecEvent(body) => collect_vtec_event_rows(prepared, body)?,
        ProductBody::Generic(body) => collect_generic_rows(prepared, None, body)?,
    }
    Ok(())
}

fn collect_vtec_event_rows(
    prepared: &mut PreparedProduct,
    body: &VtecEventBody,
) -> PersistResult<()> {
    for segment in &body.segments {
        let segment_index = Some(usize_to_i32(segment.segment_index, "segment_index")?);
        for vtec in &segment.vtec {
            push_vtec_row(&mut prepared.vtec, segment_index, vtec);
        }
        for (hvtec_index, hvtec) in segment.hvtec.iter().enumerate() {
            push_hvtec_row(
                &mut prepared.hvtec,
                &mut prepared.search_points,
                segment_index,
                usize_to_i32(hvtec_index, "hvtec_index")?,
                hvtec,
            )?;
        }
        for (polygon_index, polygon) in segment.polygons.iter().enumerate() {
            prepared.polygons.push(ProductPolygonRow {
                segment_index,
                polygon_index: usize_to_i32(polygon_index, "polygon_index")?,
                polygon_wkt: polygon.wkt.clone(),
            });
        }
        for (entry_index, entry) in segment.time_mot_loc.iter().enumerate() {
            push_time_mot_loc_row(
                &mut prepared.time_mot_loc,
                &mut prepared.search_points,
                segment_index,
                usize_to_i32(entry_index, "time_mot_loc_index")?,
                entry,
            )?;
        }
        for (entry_index, entry) in segment.wind_hail.iter().enumerate() {
            prepared.wind_hail.push(ProductWindHailRow {
                segment_index,
                entry_index: usize_to_i32(entry_index, "wind_hail_index")?,
                kind: serde_label(&entry.kind)?,
                numeric_value: entry.numeric_value,
                units: entry.units.clone(),
                comparison: entry.comparison.map(|value| value.to_string()),
            });
        }
        collect_ugc_rows(
            &mut prepared.ugc_areas,
            &mut prepared.search_points,
            segment_index,
            &segment.ugc_sections,
        )?;
    }
    Ok(())
}

fn collect_generic_rows(
    prepared: &mut PreparedProduct,
    segment_index: Option<i32>,
    body: &GenericBody,
) -> PersistResult<()> {
    if let Some(sections) = body.ugc.as_ref() {
        collect_ugc_rows(
            &mut prepared.ugc_areas,
            &mut prepared.search_points,
            segment_index,
            sections,
        )?;
    }
    if let Some(polygons) = body.latlon.as_ref() {
        for (polygon_index, polygon) in polygons.iter().enumerate() {
            prepared.polygons.push(ProductPolygonRow {
                segment_index,
                polygon_index: usize_to_i32(polygon_index, "polygon_index")?,
                polygon_wkt: polygon.wkt.clone(),
            });
        }
    }
    if let Some(entries) = body.time_mot_loc.as_ref() {
        for (entry_index, entry) in entries.iter().enumerate() {
            push_time_mot_loc_row(
                &mut prepared.time_mot_loc,
                &mut prepared.search_points,
                segment_index,
                usize_to_i32(entry_index, "time_mot_loc_index")?,
                entry,
            )?;
        }
    }
    if let Some(entries) = body.wind_hail.as_ref() {
        for (entry_index, entry) in entries.iter().enumerate() {
            prepared.wind_hail.push(ProductWindHailRow {
                segment_index,
                entry_index: usize_to_i32(entry_index, "wind_hail_index")?,
                kind: serde_label(&entry.kind)?,
                numeric_value: entry.numeric_value,
                units: entry.units.clone(),
                comparison: entry.comparison.map(|value| value.to_string()),
            });
        }
    }
    Ok(())
}

fn collect_ugc_rows(
    target: &mut Vec<ProductUgcAreaRow>,
    search_points: &mut Vec<ProductSearchPointRow>,
    segment_index: Option<i32>,
    sections: &[UgcSection],
) -> PersistResult<()> {
    for (section_index, section) in sections.iter().enumerate() {
        let section_index = usize_to_i32(section_index, "ugc_section_index")?;
        for spec in [
            UgcBucketSpec {
                area_kind: "county",
                bucket: &section.counties,
                code_prefix: 'C',
            },
            UgcBucketSpec {
                area_kind: "zone",
                bucket: &section.zones,
                code_prefix: 'Z',
            },
            UgcBucketSpec {
                area_kind: "fire_zone",
                bucket: &section.fire_zones,
                code_prefix: 'F',
            },
            UgcBucketSpec {
                area_kind: "marine_zone",
                bucket: &section.marine_zones,
                code_prefix: 'M',
            },
        ] {
            push_ugc_bucket(
                target,
                search_points,
                segment_index,
                section_index,
                section,
                spec,
            );
        }
    }
    Ok(())
}

fn push_ugc_bucket(
    target: &mut Vec<ProductUgcAreaRow>,
    search_points: &mut Vec<ProductSearchPointRow>,
    segment_index: Option<i32>,
    section_index: i32,
    section: &UgcSection,
    spec: UgcBucketSpec<'_>,
) {
    for (state, areas) in spec.bucket {
        for area in areas {
            let ugc_code = format!("{state}{}{:03}", spec.code_prefix, area.id);
            target.push(ProductUgcAreaRow {
                segment_index,
                section_index,
                area_kind: spec.area_kind.to_string(),
                state: state.clone(),
                ugc_code,
                name: area.name.map(str::to_string),
                expires_utc: section.expires,
                latitude: area.lat,
                longitude: area.lon,
            });
            if let (Some(latitude), Some(longitude)) = (area.lat, area.lon) {
                search_points.push(ProductSearchPointRow {
                    source_kind: spec.area_kind.to_string(),
                    source_index: section_index,
                    latitude,
                    longitude,
                });
            }
        }
    }
}

fn push_vtec_row(target: &mut Vec<ProductVtecRow>, segment_index: Option<i32>, vtec: &VtecCode) {
    target.push(ProductVtecRow {
        segment_index,
        status: vtec.status.to_string(),
        action: vtec.action.clone(),
        office: vtec.office.clone(),
        phenomena: vtec.phenomena.clone(),
        significance: vtec.significance.to_string(),
        etn: i64::from(vtec.etn),
        begin_utc: vtec.begin,
        end_utc: vtec.end,
    });
}

fn push_hvtec_row(
    target: &mut Vec<ProductHvtecRow>,
    search_points: &mut Vec<ProductSearchPointRow>,
    segment_index: Option<i32>,
    hvtec_index: i32,
    hvtec: &HvtecCode,
) -> PersistResult<()> {
    let severity = serde_label(&hvtec.severity)?;
    let cause = serde_label(&hvtec.cause)?;
    let record = serde_label(&hvtec.record)?;
    let latitude = hvtec.location.as_ref().map(|location| location.latitude);
    let longitude = hvtec.location.as_ref().map(|location| location.longitude);

    target.push(ProductHvtecRow {
        segment_index,
        hvtec_index,
        nwslid: hvtec.nwslid.clone(),
        location_name: hvtec
            .location
            .as_ref()
            .map(|location| location.place_name.to_string()),
        severity: severity.clone(),
        cause: cause.clone(),
        record: record.clone(),
        begin_utc: hvtec.begin,
        crest_utc: hvtec.crest,
        end_utc: hvtec.end,
        latitude,
        longitude,
    });

    if let (Some(latitude), Some(longitude)) = (latitude, longitude) {
        search_points.push(ProductSearchPointRow {
            source_kind: "hvtec".to_string(),
            source_index: hvtec_index,
            latitude,
            longitude,
        });
    }

    Ok(())
}

fn push_time_mot_loc_row(
    target: &mut Vec<ProductTimeMotLocRow>,
    search_points: &mut Vec<ProductSearchPointRow>,
    segment_index: Option<i32>,
    entry_index: i32,
    entry: &TimeMotLocEntry,
) -> PersistResult<()> {
    target.push(ProductTimeMotLocRow {
        segment_index,
        entry_index,
        time_utc: entry.time_utc,
        direction_degrees: i32::from(entry.direction_degrees),
        speed_kt: i32::from(entry.speed_kt),
        path_wkt: entry.wkt.clone(),
    });

    for point in &entry.points {
        search_points.push(ProductSearchPointRow {
            source_kind: "time_mot_loc".to_string(),
            source_index: entry_index,
            latitude: point.0,
            longitude: point.1,
        });
    }

    Ok(())
}

async fn upsert_product(
    tx: &mut Transaction<'_, Postgres>,
    prepared: &PreparedProduct,
) -> PersistResult<i64> {
    let row = &prepared.row;
    let product_id = sqlx::query(
        "INSERT INTO products (
            filename,
            source_timestamp_utc,
            source_receiver,
            source_message_id,
            size_bytes,
            payload_storage_kind,
            payload_location,
            metadata_storage_kind,
            metadata_location,
            source,
            family,
            artifact_kind,
            title,
            container,
            pil,
            wmo_prefix,
            bbb_kind,
            office_code,
            office_city,
            office_state,
            header_kind,
            ttaaii,
            cccc,
            ddhhmm,
            bbb,
            afos,
            has_body,
            has_artifact,
            has_issues,
            has_vtec,
            has_ugc,
            has_hvtec,
            has_latlon,
            has_time_mot_loc,
            has_wind_hail,
            vtec_count,
            ugc_count,
            hvtec_count,
            latlon_count,
            time_mot_loc_count,
            wind_hail_count,
            issue_count,
            states,
            ugc_codes,
            product_json
        ) VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
            $11, $12, $13, $14, $15, $16, $17, $18, $19, $20,
            $21, $22, $23, $24, $25, $26, $27, $28, $29, $30,
            $31, $32, $33, $34, $35, $36, $37, $38, $39, $40,
            $41, $42, $43, $44, $45
        ) ON CONFLICT (filename, source_timestamp_utc) DO UPDATE SET
            source_receiver = EXCLUDED.source_receiver,
            source_message_id = EXCLUDED.source_message_id,
            ingested_at = now(),
            size_bytes = EXCLUDED.size_bytes,
            payload_storage_kind = EXCLUDED.payload_storage_kind,
            payload_location = EXCLUDED.payload_location,
            metadata_storage_kind = EXCLUDED.metadata_storage_kind,
            metadata_location = EXCLUDED.metadata_location,
            source = EXCLUDED.source,
            family = EXCLUDED.family,
            artifact_kind = EXCLUDED.artifact_kind,
            title = EXCLUDED.title,
            container = EXCLUDED.container,
            pil = EXCLUDED.pil,
            wmo_prefix = EXCLUDED.wmo_prefix,
            bbb_kind = EXCLUDED.bbb_kind,
            office_code = EXCLUDED.office_code,
            office_city = EXCLUDED.office_city,
            office_state = EXCLUDED.office_state,
            header_kind = EXCLUDED.header_kind,
            ttaaii = EXCLUDED.ttaaii,
            cccc = EXCLUDED.cccc,
            ddhhmm = EXCLUDED.ddhhmm,
            bbb = EXCLUDED.bbb,
            afos = EXCLUDED.afos,
            has_body = EXCLUDED.has_body,
            has_artifact = EXCLUDED.has_artifact,
            has_issues = EXCLUDED.has_issues,
            has_vtec = EXCLUDED.has_vtec,
            has_ugc = EXCLUDED.has_ugc,
            has_hvtec = EXCLUDED.has_hvtec,
            has_latlon = EXCLUDED.has_latlon,
            has_time_mot_loc = EXCLUDED.has_time_mot_loc,
            has_wind_hail = EXCLUDED.has_wind_hail,
            vtec_count = EXCLUDED.vtec_count,
            ugc_count = EXCLUDED.ugc_count,
            hvtec_count = EXCLUDED.hvtec_count,
            latlon_count = EXCLUDED.latlon_count,
            time_mot_loc_count = EXCLUDED.time_mot_loc_count,
            wind_hail_count = EXCLUDED.wind_hail_count,
            issue_count = EXCLUDED.issue_count,
            states = EXCLUDED.states,
            ugc_codes = EXCLUDED.ugc_codes,
            product_json = EXCLUDED.product_json
        RETURNING id",
    )
    .bind(&row.filename)
    .bind(row.source_timestamp_utc)
    .bind(&row.source_receiver)
    .bind(&row.source_message_id)
    .bind(row.size_bytes)
    .bind(&row.payload_storage_kind)
    .bind(&row.payload_location)
    .bind(&row.metadata_storage_kind)
    .bind(&row.metadata_location)
    .bind(&row.source)
    .bind(&row.family)
    .bind(&row.artifact_kind)
    .bind(&row.title)
    .bind(&row.container)
    .bind(&row.pil)
    .bind(&row.wmo_prefix)
    .bind(&row.bbb_kind)
    .bind(&row.office_code)
    .bind(&row.office_city)
    .bind(&row.office_state)
    .bind(&row.header_kind)
    .bind(&row.ttaaii)
    .bind(&row.cccc)
    .bind(&row.ddhhmm)
    .bind(&row.bbb)
    .bind(&row.afos)
    .bind(row.has_body)
    .bind(row.has_artifact)
    .bind(row.has_issues)
    .bind(row.has_vtec)
    .bind(row.has_ugc)
    .bind(row.has_hvtec)
    .bind(row.has_latlon)
    .bind(row.has_time_mot_loc)
    .bind(row.has_wind_hail)
    .bind(row.vtec_count)
    .bind(row.ugc_count)
    .bind(row.hvtec_count)
    .bind(row.latlon_count)
    .bind(row.time_mot_loc_count)
    .bind(row.wind_hail_count)
    .bind(row.issue_count)
    .bind(&row.states)
    .bind(&row.ugc_codes)
    .bind(&row.product_json)
    .fetch_one(&mut **tx)
    .await?
    .get::<i64, _>("id");

    Ok(product_id)
}

async fn replace_children(
    tx: &mut Transaction<'_, Postgres>,
    product_id: i64,
    prepared: &PreparedProduct,
) -> PersistResult<()> {
    for table in [
        "product_issues",
        "product_vtec",
        "product_ugc_areas",
        "product_hvtec",
        "product_time_mot_loc",
        "product_polygons",
        "product_wind_hail",
        "product_search_points",
    ] {
        let query = format!("DELETE FROM {table} WHERE product_id = $1");
        sqlx::query(&query)
            .bind(product_id)
            .execute(&mut **tx)
            .await?;
    }

    insert_product_issues(tx, product_id, &prepared.issues).await?;
    insert_product_vtec(tx, product_id, &prepared.vtec).await?;
    insert_product_ugc_areas(tx, product_id, &prepared.ugc_areas).await?;
    insert_product_hvtec(tx, product_id, &prepared.hvtec).await?;
    insert_product_time_mot_loc(tx, product_id, &prepared.time_mot_loc).await?;
    insert_product_polygons(tx, product_id, &prepared.polygons).await?;
    insert_product_wind_hail(tx, product_id, &prepared.wind_hail).await?;
    insert_product_search_points(tx, product_id, &prepared.search_points).await?;
    Ok(())
}

async fn insert_product_issues(
    tx: &mut Transaction<'_, Postgres>,
    product_id: i64,
    rows: &[ProductIssueRow],
) -> PersistResult<()> {
    if rows.is_empty() {
        return Ok(());
    }
    let mut builder = QueryBuilder::<Postgres>::new(
        "INSERT INTO product_issues (product_id, kind, code, message, line) ",
    );
    builder.push_values(rows, |mut row, issue| {
        row.push_bind(product_id)
            .push_bind(&issue.kind)
            .push_bind(&issue.code)
            .push_bind(&issue.message)
            .push_bind(&issue.line);
    });
    builder.build().execute(&mut **tx).await?;
    Ok(())
}

async fn insert_product_vtec(
    tx: &mut Transaction<'_, Postgres>,
    product_id: i64,
    rows: &[ProductVtecRow],
) -> PersistResult<()> {
    if rows.is_empty() {
        return Ok(());
    }
    let mut builder = QueryBuilder::<Postgres>::new(
        "INSERT INTO product_vtec (product_id, segment_index, status, action, office, phenomena, significance, etn, begin_utc, end_utc) ",
    );
    builder.push_values(rows, |mut row, vtec| {
        row.push_bind(product_id)
            .push_bind(vtec.segment_index)
            .push_bind(&vtec.status)
            .push_bind(&vtec.action)
            .push_bind(&vtec.office)
            .push_bind(&vtec.phenomena)
            .push_bind(&vtec.significance)
            .push_bind(vtec.etn)
            .push_bind(vtec.begin_utc)
            .push_bind(vtec.end_utc);
    });
    builder.build().execute(&mut **tx).await?;
    Ok(())
}

async fn insert_product_ugc_areas(
    tx: &mut Transaction<'_, Postgres>,
    product_id: i64,
    rows: &[ProductUgcAreaRow],
) -> PersistResult<()> {
    if rows.is_empty() {
        return Ok(());
    }
    let mut builder = QueryBuilder::<Postgres>::new(
        "INSERT INTO product_ugc_areas (product_id, segment_index, section_index, area_kind, state, ugc_code, name, expires_utc, latitude, longitude, point_geom) ",
    );
    builder.push_values(rows, |mut row, area| {
        row.push_bind(product_id)
            .push_bind(area.segment_index)
            .push_bind(area.section_index)
            .push_bind(&area.area_kind)
            .push_bind(&area.state)
            .push_bind(&area.ugc_code)
            .push_bind(&area.name)
            .push_bind(area.expires_utc)
            .push_bind(area.latitude)
            .push_bind(area.longitude)
            .push(if area.latitude.is_some() && area.longitude.is_some() {
                "ST_SetSRID(ST_MakePoint("
            } else {
                "NULL"
            });
        if let (Some(latitude), Some(longitude)) = (area.latitude, area.longitude) {
            row.push_bind_unseparated(longitude)
                .push_unseparated(", ")
                .push_bind_unseparated(latitude)
                .push_unseparated("), 4326)");
        }
    });
    builder.build().execute(&mut **tx).await?;
    Ok(())
}

async fn insert_product_hvtec(
    tx: &mut Transaction<'_, Postgres>,
    product_id: i64,
    rows: &[ProductHvtecRow],
) -> PersistResult<()> {
    if rows.is_empty() {
        return Ok(());
    }
    let mut builder = QueryBuilder::<Postgres>::new(
        "INSERT INTO product_hvtec (product_id, segment_index, hvtec_index, nwslid, location_name, severity, cause, record, begin_utc, crest_utc, end_utc, latitude, longitude, point_geom) ",
    );
    builder.push_values(rows, |mut row, hvtec| {
        row.push_bind(product_id)
            .push_bind(hvtec.segment_index)
            .push_bind(hvtec.hvtec_index)
            .push_bind(&hvtec.nwslid)
            .push_bind(&hvtec.location_name)
            .push_bind(&hvtec.severity)
            .push_bind(&hvtec.cause)
            .push_bind(&hvtec.record)
            .push_bind(hvtec.begin_utc)
            .push_bind(hvtec.crest_utc)
            .push_bind(hvtec.end_utc)
            .push_bind(hvtec.latitude)
            .push_bind(hvtec.longitude)
            .push(if hvtec.latitude.is_some() && hvtec.longitude.is_some() {
                "ST_SetSRID(ST_MakePoint("
            } else {
                "NULL"
            });
        if let (Some(latitude), Some(longitude)) = (hvtec.latitude, hvtec.longitude) {
            row.push_bind_unseparated(longitude)
                .push_unseparated(", ")
                .push_bind_unseparated(latitude)
                .push_unseparated("), 4326)");
        }
    });
    builder.build().execute(&mut **tx).await?;
    Ok(())
}

async fn insert_product_time_mot_loc(
    tx: &mut Transaction<'_, Postgres>,
    product_id: i64,
    rows: &[ProductTimeMotLocRow],
) -> PersistResult<()> {
    if rows.is_empty() {
        return Ok(());
    }
    let mut builder = QueryBuilder::<Postgres>::new(
        "INSERT INTO product_time_mot_loc (product_id, segment_index, entry_index, time_utc, direction_degrees, speed_kt, path_wkt, path_geom) ",
    );
    builder.push_values(rows, |mut row, entry| {
        row.push_bind(product_id)
            .push_bind(entry.segment_index)
            .push_bind(entry.entry_index)
            .push_bind(entry.time_utc)
            .push_bind(entry.direction_degrees)
            .push_bind(entry.speed_kt)
            .push_bind(&entry.path_wkt)
            .push("ST_GeomFromText(")
            .push_bind_unseparated(&entry.path_wkt)
            .push_unseparated(", 4326)");
    });
    builder.build().execute(&mut **tx).await?;
    Ok(())
}

async fn insert_product_polygons(
    tx: &mut Transaction<'_, Postgres>,
    product_id: i64,
    rows: &[ProductPolygonRow],
) -> PersistResult<()> {
    if rows.is_empty() {
        return Ok(());
    }
    let mut builder = QueryBuilder::<Postgres>::new(
        "INSERT INTO product_polygons (product_id, segment_index, polygon_index, polygon_wkt, polygon_geom) ",
    );
    builder.push_values(rows, |mut row, polygon| {
        row.push_bind(product_id)
            .push_bind(polygon.segment_index)
            .push_bind(polygon.polygon_index)
            .push_bind(&polygon.polygon_wkt)
            .push("ST_GeomFromText(")
            .push_bind_unseparated(&polygon.polygon_wkt)
            .push_unseparated(", 4326)");
    });
    builder.build().execute(&mut **tx).await?;
    Ok(())
}

async fn insert_product_wind_hail(
    tx: &mut Transaction<'_, Postgres>,
    product_id: i64,
    rows: &[ProductWindHailRow],
) -> PersistResult<()> {
    if rows.is_empty() {
        return Ok(());
    }
    let mut builder = QueryBuilder::<Postgres>::new(
        "INSERT INTO product_wind_hail (product_id, segment_index, entry_index, kind, numeric_value, units, comparison) ",
    );
    builder.push_values(rows, |mut row, entry| {
        row.push_bind(product_id)
            .push_bind(entry.segment_index)
            .push_bind(entry.entry_index)
            .push_bind(&entry.kind)
            .push_bind(entry.numeric_value)
            .push_bind(&entry.units)
            .push_bind(&entry.comparison);
    });
    builder.build().execute(&mut **tx).await?;
    Ok(())
}

async fn insert_product_search_points(
    tx: &mut Transaction<'_, Postgres>,
    product_id: i64,
    rows: &[ProductSearchPointRow],
) -> PersistResult<()> {
    if rows.is_empty() {
        return Ok(());
    }
    let mut builder = QueryBuilder::<Postgres>::new(
        "INSERT INTO product_search_points (product_id, source_kind, source_index, latitude, longitude, point_geom) ",
    );
    builder.push_values(rows, |mut row, point| {
        row.push_bind(product_id)
            .push_bind(&point.source_kind)
            .push_bind(point.source_index)
            .push_bind(point.latitude)
            .push_bind(point.longitude)
            .push("ST_SetSRID(ST_MakePoint(")
            .push_bind_unseparated(point.longitude)
            .push_unseparated(", ")
            .push_bind_unseparated(point.latitude)
            .push_unseparated("), 4326)");
    });
    builder.build().execute(&mut **tx).await?;
    Ok(())
}

fn flatten_header(header: Option<&ProductHeaderV2>) -> HeaderColumns {
    match header {
        Some(ProductHeaderV2::Afos {
            ttaaii,
            cccc,
            ddhhmm,
            bbb,
            afos,
        }) => HeaderColumns {
            header_kind: Some("afos".to_string()),
            ttaaii: Some(ttaaii.clone()),
            cccc: Some(cccc.clone()),
            ddhhmm: Some(ddhhmm.clone()),
            bbb: bbb.clone(),
            afos: Some(afos.clone()),
        },
        Some(ProductHeaderV2::Wmo {
            ttaaii,
            cccc,
            ddhhmm,
            bbb,
        }) => HeaderColumns {
            header_kind: Some("wmo".to_string()),
            ttaaii: Some(ttaaii.clone()),
            cccc: Some(cccc.clone()),
            ddhhmm: Some(ddhhmm.clone()),
            bbb: bbb.clone(),
            afos: None,
        },
        None => HeaderColumns {
            header_kind: None,
            ttaaii: None,
            cccc: None,
            ddhhmm: None,
            bbb: None,
            afos: None,
        },
    }
}

fn source_receiver(origin: &ProductOrigin) -> &'static str {
    match origin {
        ProductOrigin::Qbt => "qbt",
        ProductOrigin::WxWire { .. } => "wxwire",
        _ => "unknown",
    }
}

fn source_message_id(origin: &ProductOrigin) -> Option<String> {
    match origin {
        ProductOrigin::Qbt => None,
        ProductOrigin::WxWire { message_id, .. } => Some(message_id.clone()),
        _ => None,
    }
}

fn blob_storage_kind(kind: BlobStorageKind) -> &'static str {
    match kind {
        BlobStorageKind::Filesystem => "filesystem",
    }
}

fn find_blob(blobs: &[StoredBlob], role: BlobRole) -> PersistResult<&StoredBlob> {
    find_blob_optional(blobs, role).ok_or_else(|| {
        PersistError::InvalidRequest(format!("missing required `{role:?}` blob reference"))
    })
}

fn find_blob_optional(blobs: &[StoredBlob], role: BlobRole) -> Option<&StoredBlob> {
    blobs.iter().find(|blob| blob.role == role)
}

fn usize_to_i32(value: usize, field: &str) -> PersistResult<i32> {
    i32::try_from(value).map_err(|_| {
        PersistError::InvalidRequest(format!("{field} value `{value}` exceeds i32 range"))
    })
}

fn serde_label<T: Serialize>(value: &T) -> PersistResult<String> {
    match serde_json::to_value(value)? {
        Value::String(value) => Ok(value),
        Value::Number(value) => Ok(value.to_string()),
        Value::Bool(value) => Ok(value.to_string()),
        other => Err(PersistError::InvalidRequest(format!(
            "expected scalar label, found {other}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::{PreparedProduct, find_blob, serde_label};
    use crate::{BlobRole, BlobStorageKind, CompletedFileMetadata, StoredBlob};
    use emwin_protocol::ingest::ProductOrigin;

    #[test]
    fn serde_label_uses_serde_names() {
        assert_eq!(
            serde_label(&emwin_parser::ProductEnrichmentSource::TextHeader)
                .expect("label should serialize"),
            "text_header"
        );
        assert_eq!(
            serde_label(&emwin_parser::WindHailKind::MaxWindGust).expect("label should serialize"),
            "max_wind_gust"
        );
    }

    #[test]
    fn prepared_product_extracts_blob_roles_and_summary_fields() {
        let metadata = CompletedFileMetadata::build(
            "AFDBOX.TXT",
            1704070800,
            ProductOrigin::Qbt,
            b"000 \nFXUS61 KBOX 022101\nAFDBOX\nBody\n",
        );
        let blobs = vec![
            StoredBlob {
                kind: BlobStorageKind::Filesystem,
                role: BlobRole::Payload,
                location: "/tmp/AFDBOX.TXT".to_string(),
                size_bytes: 1,
                content_type: Some("application/octet-stream".to_string()),
            },
            StoredBlob {
                kind: BlobStorageKind::Filesystem,
                role: BlobRole::MetadataSidecar,
                location: "/tmp/AFDBOX.JSON".to_string(),
                size_bytes: 1,
                content_type: Some("application/json".to_string()),
            },
        ];

        let prepared = PreparedProduct::prepare(&metadata, &blobs).expect("product should prepare");
        assert_eq!(prepared.row.payload_location, "/tmp/AFDBOX.TXT");
        assert_eq!(
            prepared.row.metadata_location.as_deref(),
            Some("/tmp/AFDBOX.JSON")
        );
        assert_eq!(prepared.row.source_receiver, "qbt");
        assert_eq!(prepared.row.source_message_id, None);
        assert_eq!(prepared.row.source, "text_header");
        assert_eq!(prepared.row.container, "raw");
    }

    #[test]
    fn missing_payload_blob_is_rejected() {
        let err = find_blob(
            &[StoredBlob {
                kind: BlobStorageKind::Filesystem,
                role: BlobRole::MetadataSidecar,
                location: "/tmp/AFDBOX.JSON".to_string(),
                size_bytes: 1,
                content_type: Some("application/json".to_string()),
            }],
            BlobRole::Payload,
        )
        .expect_err("payload is required");

        assert!(err.to_string().contains("Payload"));
    }
}
