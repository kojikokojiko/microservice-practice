CREATE TABLE IF NOT EXISTS student.submissions (
    id UUID PRIMARY KEY,
    assignment_id UUID NOT NULL,
    student_id TEXT NOT NULL,
    content TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
