/**
 * Factory for creating execution orchestrators
 * 
 * Provides a simple API to create orchestrators for different platforms:
 * - local: LocalOrchestrator (runs tasks on local machine)
 * - cloudrun: CloudRunOrchestrator (runs tasks on Google Cloud Run)
 * - Future: aws-lambda, kubernetes, etc.
 */

import type { Logger } from 'pino';
import type { ExecutionOrchestrator } from './orchestrator.js';
import { LocalOrchestrator } from './local-orchestrator.js';
import { CloudRunOrchestrator, type CloudRunConfig } from './cloudrun-orchestrator.js';

/**
 * Configuration for creating an orchestrator
 */
export interface OrchestratorConfig {
  type: 'local' | 'cloudrun';
  cloudrun?: CloudRunConfig;
}

/**
 * Creates an execution orchestrator based on configuration
 * 
 * @param config Orchestrator configuration
 * @param logger Pino logger instance
 * @returns Execution orchestrator instance
 * 
 * @example
 * // Local orchestrator
 * const local = createOrchestrator({ type: 'local' }, logger);
 * 
 * // Cloud Run orchestrator
 * const cloudrun = createOrchestrator({
 *   type: 'cloudrun',
 *   cloudrun: {
 *     projectId: 'my-project',
 *     region: 'us-east5',
 *     image: 'us-east5-docker.pkg.dev/my-project/repo/image:latest'
 *   }
 * }, logger);
 */
export function createOrchestrator(
  config: OrchestratorConfig,
  logger: Logger
): ExecutionOrchestrator {
  switch (config.type) {
    case 'local':
      return new LocalOrchestrator();

    case 'cloudrun':
      if (!config.cloudrun) {
        throw new Error('Cloud Run configuration required when type is "cloudrun"');
      }
      return new CloudRunOrchestrator(config.cloudrun, logger);

    default:
      throw new Error(`Unknown orchestrator type: ${(config as any).type}`);
  }
}
