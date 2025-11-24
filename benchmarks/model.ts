export type Task = {
  before_run: Array<string>;
  run: { command: string; parallelism?: number; timeout?: number; early_exit?: boolean };
  validations?: Array<Validation>;
  sources: Array<Source>;
};

export type Validation = {
  name: string;
  type: "matches_regex";
  regex: string;
};

export type Source = { csv: string } | { cmd: string } | { value: Record<string, string>[] };


export enum TaskStatus {
  Passed = "passed",
  ValidationFailed = "validation_failed",
  Timeout = "timeout",
  Failed = "failed",
}
