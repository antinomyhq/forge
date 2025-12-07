export type Task = {
  before_run: Array<string>;
  run: string | Array<string>;
  parallelism?: number;
  timeout?: number;
  early_exit?: boolean;
  validations?: Array<Validation>;
  sources: Array<Source>;
  cwd?: string;
  eval_dir?: string; // Set by CLI after loading task.yml
};

export type Validation = 
  | {
      name: string;
      type: "regex";
      regex: string;
    }
  | {
      name: string;
      type: "shell";
      command: string;
      exit_code?: number;
    };

export type Source = { csv: string } | { cmd: string } | { value: Record<string, string>[] };

export type ValidationResult = {
  name: string;
  passed: boolean;
  message?: string;
  duration?: number;
};

export enum TaskStatus {
  Passed = "passed",
  ValidationFailed = "validation_failed",
  Timeout = "timeout",
  Failed = "failed",
}
