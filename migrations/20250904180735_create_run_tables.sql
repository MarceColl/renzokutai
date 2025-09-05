CREATE TABLE IF NOT EXISTS pipeline_runs (
	id INTEGER PRIMARY KEY NOT NULL,
	pipeline_name TEXT NOT NULL,
	step_name TEXT NOT NULL,
	started_at TIMESTAMP,
	finished_at TIMESTAMP
);

CREATE TABLE IF NOT EXISTS pipeline_run_logs (
	id INTEGER PRIMARY KEY NOT NULL,
	pipeline_run_id INTEGER NOT NULL,
	log_idx INTEGER NOT NULL,
	textlog TEXT NOT NULL,
	FOREIGN KEY (pipeline_run_id) REFERENES pipeline_runs(id)
);
