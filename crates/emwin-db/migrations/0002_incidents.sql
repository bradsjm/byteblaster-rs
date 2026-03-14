CREATE TABLE IF NOT EXISTS incidents (
    office TEXT NOT NULL,
    phenomena TEXT NOT NULL,
    significance TEXT NOT NULL,
    etn BIGINT NOT NULL,
    current_status TEXT NOT NULL,
    latest_vtec_action TEXT NOT NULL,
    issued_at TIMESTAMPTZ NOT NULL,
    start_utc TIMESTAMPTZ,
    end_utc TIMESTAMPTZ,
    last_updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    first_product_id BIGINT NOT NULL REFERENCES products(id),
    latest_product_id BIGINT NOT NULL REFERENCES products(id),
    latest_product_timestamp_utc TIMESTAMPTZ NOT NULL,
    CONSTRAINT incidents_pkey PRIMARY KEY (office, phenomena, significance, etn),
    CONSTRAINT incidents_current_status_check CHECK (
        current_status IN ('active', 'cancelled', 'expired', 'upgraded')
    )
);

CREATE INDEX IF NOT EXISTS incidents_current_status_end_utc_idx
    ON incidents (current_status, end_utc);
