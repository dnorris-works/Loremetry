import type { ModelInfo } from '../types';

export function hasModelPricing(model: ModelInfo): boolean {
  return model.input_price != null
    && model.output_price != null
    && model.input_price >= 0
    && model.output_price >= 0;
}

export function findPricedModel(modelId: string, models: ModelInfo[]): ModelInfo | undefined {
  if (!modelId) return undefined;
  const model = models.find(m => m.id === modelId);
  return model && hasModelPricing(model) ? model : undefined;
}

export interface ResolvedModelPricing {
  input_price: number;
  output_price: number;
  available: boolean;
}

/** Prices for the assigned model only — from the fetched TokenMix catalog. */
export function resolveModelPrices(modelId: string, models: ModelInfo[]): ResolvedModelPricing {
  const model = findPricedModel(modelId, models);
  if (!model) {
    return { input_price: 0, output_price: 0, available: false };
  }

  return {
    input_price: model.input_price!,
    output_price: model.output_price!,
    available: true,
  };
}

export function isAiConfigured(apiKey: string, defaultModel: string): boolean {
  return apiKey.trim().length > 0 && defaultModel.trim().length > 0;
}
