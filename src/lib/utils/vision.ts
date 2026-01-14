/**
 * Vision Support Detection
 * 
 * Utilities to detect if an agent/model supports vision/image input
 */

import type { Agent, AgentProvider } from '$lib/types';

/**
 * Known vision-capable model patterns by provider
 */
const VISION_PATTERNS: Record<AgentProvider, RegExp[]> = {
    // llama_server: check mmproj_path separately
    llama_server: [],

    // Ollama vision models
    ollama: [
        /llava/i,
        /bakllava/i,
        /moondream/i,
        /cogvlm/i,
        /minicpm-v/i,
        /llama3\.2-vision/i,
        /llama-3\.2.*vision/i,
        /gemma3/i,
        /gemma-3/i,
    ],

    // OpenRouter vision models
    open_router: [
        /vision/i,
        /gpt-4o/i,
        /gpt-4-turbo/i,
        /claude-3/i,
        /gemini/i,
        /pixtral/i,
        /llava/i,
        /qwen.*vl/i,
        /qwen2-vl/i,
    ],

    // OpenAI vision models
    open_ai: [
        /gpt-4o/i,
        /gpt-4-turbo/i,
        /gpt-4-vision/i,
        /o1/i,
    ],

    // Anthropic - all Claude 3+ support vision
    anthropic: [
        /claude-3/i,
        /claude-3\.5/i,
    ],

    // Custom - no auto-detection
    custom: [],
};

/**
 * Check if an agent supports vision/image input
 */
export function supportsVision(agent: Agent): boolean {
    // For llama_server, check if mmproj_path is set
    if (agent.provider === 'llama_server') {
        return !!agent.mmproj_path;
    }

    // Get the model identifier to check
    const modelId = agent.model_id || agent.model_path || '';
    
    if (!modelId) {
        return false;
    }

    // Check against known patterns for the provider
    const patterns = VISION_PATTERNS[agent.provider] || [];
    
    return patterns.some(pattern => pattern.test(modelId));
}

/**
 * Get a human-readable label for vision support
 */
export function getVisionLabel(agent: Agent): string | null {
    if (supportsVision(agent)) {
        return 'Vision';
    }
    return null;
}
