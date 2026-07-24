import { Extension } from '@tiptap/core';
import type { EditorState } from '@tiptap/pm/state';
import { Plugin, PluginKey } from '@tiptap/pm/state';
import { Decoration, DecorationSet } from '@tiptap/pm/view';
import { highlightToDocRange, type StyleCoachHighlight } from './styleCoach';

export const styleCoachPluginKey = new PluginKey('styleCoachHighlights');

export interface StyleCoachExtensionOptions {
  getHighlights: () => StyleCoachHighlight[];
  enabled: () => boolean;
}

function buildDecorations(state: EditorState, opts: StyleCoachExtensionOptions): DecorationSet {
  if (!opts.enabled()) return DecorationSet.empty;

  const highlights = opts.getHighlights();
  if (highlights.length === 0) return DecorationSet.empty;

  const decorations: Decoration[] = [];

  for (const h of highlights) {
    const range = highlightToDocRange(state.doc, h);
    if (!range) continue;

    decorations.push(
      Decoration.inline(range.from, range.to, {
        class: `coach-mark coach-${h.type}`,
        'data-coach': h.type,
      }),
    );
  }

  return decorations.length > 0
    ? DecorationSet.create(state.doc, decorations)
    : DecorationSet.empty;
}

export const StyleCoachMarks = Extension.create<StyleCoachExtensionOptions>({
  name: 'styleCoachMarks',

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
        key: styleCoachPluginKey,
        state: {
          init: (_, state) => buildDecorations(state, opts),
          apply(tr, set, _oldState, newState) {
            if (tr.docChanged || tr.getMeta(styleCoachPluginKey) !== undefined) {
              return buildDecorations(newState, opts);
            }
            return set.map(tr.mapping, newState.doc);
          },
        },
        props: {
          decorations(state) {
            return styleCoachPluginKey.getState(state) ?? DecorationSet.empty;
          },
        },
      }),
    ];
  },
});
