export type Task = {
  before_run: Array<string>;
  run: string | Array<string>;
  parallelism?: number;
  timeout?: number;
  early_exit?: boolean;
  validations?: Array<Validation>;
  sources: Array<Source>;
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
    }
  | {
      name: string;
      type: "llm";
      evaluator: string;
      criteria: Record<string, any>;
    };

export type Source = { csv: string } | { cmd: string } | { value: Record<string, string>[] };


export enum TaskStatus {
  Passed = "passed",
  ValidationFailed = "validation_failed",
  Timeout = "timeout",
  Failed = "failed",
}
