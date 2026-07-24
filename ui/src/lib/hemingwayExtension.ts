import { Extension } from '@tiptap/core';
import { Plugin, PluginKey } from '@tiptap/pm/state';
import { Decoration, DecorationSet } from '@tiptap/pm/view';
import { offsetsToDocRange, type HemingwayHighlight } from './hemingway';

export const hemingwayPluginKey = new PluginKey('hemingwayHighlights');

export interface HemingwayHighlightOptions {
  getHighlights: () => HemingwayHighlight[];
  enabled: () => boolean;
}

export const HemingwayCoach = Extension.create<HemingwayHighlightOptions>({
  name: 'hemingwayCoach',

  addOptions() {
    return {
      getHighlights: () => [],
      enabled: () => false,
    };
  },

  addProseMirrorPlugins() {
    const opts = this.options;
    return [
      new Plugin({
        key: hemingwayPluginKey,
        props: {
          decorations(state) {
            if (!opts.enabled()) return DecorationSet.empty;
            const highlights = opts.getHighlights();
            if (highlights.length === 0) return DecorationSet.empty;

            const decorations: Decoration[] = [];
            for (const h of highlights) {
              const range = offsetsToDocRange(state.doc, h.from, h.to);
              if (!range) continue;
              decorations.push(
                Decoration.inline(range.from, range.to, {
                  class: `hemingway-mark hemingway-${h.type}`,
                }),
              );
            }
            return DecorationSet.create(state.doc, decorations);
          },
        },
      }),
    ];
  },
});
