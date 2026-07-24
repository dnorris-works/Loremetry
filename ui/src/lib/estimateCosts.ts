/**
 * Client-side report cost estimates — mirrors crates/core/src/commands.rs estimate_report_costs.
 */

import type { ReportTypeDef } from '../types';
import type { ChapterWordStats } from './manuscriptCache';

const WORDS_TO_TOKENS = 1.3;
const SYSTEM_PROMPT_TOKENS = 400;

export interface ReportModelPrice {
  report_id: string;
  input_price: number;
  output_price: number;
}

export interface ReportCostEstimate {
  report_id: string;
  estimated_cost: number;
}

export function estimateReportCosts(
  reportTypes: ReportTypeDef[],
  modelPrices: ReportModelPrice[],
  stats: ChapterWordStats,
): ReportCostEstimate[] {
  const typeById = new Map(reportTypes.map(r => [r.id, r]));
  const { chapterCount, wordCounts } = stats;

  return modelPrices.map(rp => {
    const rt = typeById.get(rp.report_id);
    const params = {
      truncation: rt?.cost_truncation ?? 4000,
      output_max: rt?.cost_output_max ?? 1000,
      per_chapter: rt?.cost_per_chapter ?? false,
      fixed_calls: rt?.cost_fixed_calls ?? 1,
    };

    if (params.output_max === 0 && params.fixed_calls === 0 && !params.per_chapter) {
      return { report_id: rp.report_id, estimated_cost: 0 };
    }

    let totalInputTokens: number;
    let totalOutputTokens: number;

    if (params.per_chapter) {
      totalInputTokens = wordCounts.reduce((sum, wc) => {
        const truncated = params.truncation > 0 ? Math.min(wc, params.truncation) : wc;
        return sum + Math.floor(truncated * WORDS_TO_TOKENS) + SYSTEM_PROMPT_TOKENS;
      }, 0);
      totalOutputTokens = chapterCount * params.output_max;
    } else {
      totalInputTokens = params.fixed_calls * (2000 + SYSTEM_PROMPT_TOKENS);
      totalOutputTokens = params.fixed_calls * params.output_max;
    }

    const cost = (totalInputTokens / 1000) * rp.input_price
      + (totalOutputTokens / 1000) * rp.output_price;

    return {
      report_id: rp.report_id,
      estimated_cost: Math.round(cost * 1000) / 1000,
    };
  });
}
