import workerpool from 'workerpool';
import { executeScenario, type ScenarioInput, type ScenarioOptions } from './analysis-dsl/scenario.ts';

async function runScenario(input: ScenarioInput, options: ScenarioOptions) {
  return executeScenario(input, options, reportUpdate => workerpool.workerEmit({ reportUpdate }));
}
workerpool.worker({ runScenario });
