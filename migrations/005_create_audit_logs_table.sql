-- Audit log for security and compliance
CREATE TABLE IF NOT EXISTS audit_logs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    event_type VARCHAR(255) NOT NULL,
    tenant VARCHAR(255),
    user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    repository_id VARCHAR(255),
    operation VARCHAR(100) NOT NULL,
    resource_type VARCHAR(100),
    resource_id VARCHAR(255),
    details JSONB,
    ip_address INET,
    user_agent TEXT,
    status VARCHAR(50) NOT NULL,
    error_message TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_audit_logs_event_type ON audit_logs(event_type);
CREATE INDEX idx_audit_logs_tenant ON audit_logs(tenant);
CREATE INDEX idx_audit_logs_user ON audit_logs(user_id);
CREATE INDEX idx_audit_logs_repository ON audit_logs(repository_id);
CREATE INDEX idx_audit_logs_status ON audit_logs(status);
CREATE INDEX idx_audit_logs_created_at ON audit_logs(created_at DESC);
CREATE INDEX idx_audit_logs_user_repo ON audit_logs(user_id, repository_id);

COMMENT ON TABLE audit_logs IS 'Security and operation audit logs for compliance';
COMMENT ON COLUMN audit_logs.details IS 'Additional event data as JSON';
COMMENT ON COLUMN audit_logs.ip_address IS 'Client IP address (if available)';
COMMENT ON COLUMN audit_logs.status IS 'Event status: success, failure, error';
