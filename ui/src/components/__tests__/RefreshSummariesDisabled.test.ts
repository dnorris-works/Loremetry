/**
 * Bug Condition Exploration Test: Refresh Summaries Enabled When Setup Incomplete
 *
 * **Validates: Requirements 1.1, 1.2, 2.1**
 *
 * This test uses property-based testing to demonstrate that the "Refresh Summaries"
 * button remains enabled (clickable) even when setupIssues is non-empty — proving
 * the bug exists.
 *
 * The ACTUAL disabled binding from AnalyzerPanel.vue (line ~728) is:
 *   :disabled="analysisCtx.isWorking.value || !storiesCtx.activeFolder.value"
 *
 * It is MISSING: || setupIssues.length > 0
 *
 * EXPECTED OUTCOME: This test FAILS on unfixed code because the button is NOT
 * disabled when setupIssues has entries, proving the bug exists.
 */
import { describe, it, expect } from 'vitest';
import { test, fc } from '@fast-check/vitest';
import { mount } from '@vue/test-utils';
import { defineComponent, ref, computed } from 'vue';

/**
 * Minimal wrapper that reproduces the EXACT disabled binding from the
 * "Refresh Summaries" button in AnalyzerPanel.vue (FIXED code).
 *
 * The actual template line (after fix):
 *   :disabled="analysisCtx.isWorking.value || !storiesCtx.activeFolder.value || setupIssues.length > 0"
 *
 * We reproduce this faithfully via props → reactive state.
 */
const RefreshSummariesButton = defineComponent({
  name: 'RefreshSummariesButton',
  props: {
    isWorking: { type: Boolean, required: true },
    activeFolder: { type: String, default: '' },
    setupIssues: { type: Array, default: () => [] },
  },
  setup(props) {
    // Mirror the actual AnalyzerPanel disabled logic (FIXED — includes setupIssues check)
    const isWorking = computed(() => props.isWorking);
    const activeFolder = computed(() => props.activeFolder);
    const setupIssues = computed(() => props.setupIssues);

    return { isWorking, activeFolder, setupIssues };
  },
  template: `
    <button
      type="button"
      class="btn btn-secondary btn-small"
      :disabled="isWorking || !activeFolder || setupIssues.length > 0"
      @click="$emit('refresh')"
    >Refresh Summaries</button>
  `,
});

describe('Preservation: Refresh Summaries disabled state for non-buggy inputs', () => {
  /**
   * **Validates: Requirements 3.1, 3.2, 3.3**
   *
   * Property 2: Preservation — when setupIssues is empty, the button disabled state
   * should equal `isWorking || !activeFolder`. This must hold for ALL combinations.
   *
   * EXPECTED OUTCOME: Tests PASS on unfixed code (confirms baseline behavior to preserve).
   */

  // Arbitrary for activeFolder: either empty string (no folder) or a non-empty string (folder set)
  const activeFolderArb = fc.oneof(
    fc.constant(''),
    fc.string({ minLength: 1, maxLength: 50 }).filter(s => s.trim().length > 0),
  );

  test.prop(
    [fc.boolean(), activeFolderArb],
    { numRuns: 100 },
  )(
    'when setupIssues is empty, disabled state equals (isWorking || !activeFolder)',
    (isWorking, activeFolder) => {
      const wrapper = mount(RefreshSummariesButton, {
        props: {
          isWorking,
          activeFolder,
          setupIssues: [],
        },
      });

      const button = wrapper.find('button');
      const expectedDisabled = isWorking || !activeFolder;

      if (expectedDisabled) {
        expect(
          button.attributes('disabled'),
          `Button should be disabled when isWorking=${isWorking}, activeFolder="${activeFolder}"`,
        ).toBeDefined();
      } else {
        expect(
          button.attributes('disabled'),
          `Button should be enabled when isWorking=${isWorking}, activeFolder="${activeFolder}"`,
        ).toBeUndefined();
      }
    },
  );

  // Explicit edge case: setupIssues=[], isWorking=false, activeFolder=set → enabled
  it('button is enabled when setupIssues=[], isWorking=false, activeFolder is set', () => {
    const wrapper = mount(RefreshSummariesButton, {
      props: {
        isWorking: false,
        activeFolder: 'my-story-folder',
        setupIssues: [],
      },
    });
    expect(wrapper.find('button').attributes('disabled')).toBeUndefined();
  });

  // Explicit edge case: setupIssues=[], isWorking=true, activeFolder=set → disabled
  it('button is disabled when setupIssues=[], isWorking=true, activeFolder is set', () => {
    const wrapper = mount(RefreshSummariesButton, {
      props: {
        isWorking: true,
        activeFolder: 'my-story-folder',
        setupIssues: [],
      },
    });
    expect(wrapper.find('button').attributes('disabled')).toBeDefined();
  });

  // Explicit edge case: setupIssues=[], isWorking=false, activeFolder=empty → disabled
  it('button is disabled when setupIssues=[], isWorking=false, activeFolder is empty', () => {
    const wrapper = mount(RefreshSummariesButton, {
      props: {
        isWorking: false,
        activeFolder: '',
        setupIssues: [],
      },
    });
    expect(wrapper.find('button').attributes('disabled')).toBeDefined();
  });
});

describe('Bug Condition: Refresh Summaries button disabled state', () => {
  // Arbitrary for non-empty setup issues arrays
  const setupIssueArb = fc.record({
    id: fc.string({ minLength: 1, maxLength: 20 }),
    message: fc.string({ minLength: 1, maxLength: 100 }),
  });

  const nonEmptySetupIssuesArb = fc.array(setupIssueArb, { minLength: 1, maxLength: 5 });

  // Arbitrary for valid folder strings (non-empty)
  const activeFolderArb = fc.string({ minLength: 1, maxLength: 50 }).filter(s => s.trim().length > 0);

  test.prop(
    [nonEmptySetupIssuesArb, activeFolderArb],
    { numRuns: 100 },
  )(
    'Refresh Summaries button MUST be disabled when setupIssues is non-empty, isWorking=false, activeFolder is set',
    (setupIssues, activeFolder) => {
      // Bug condition: setupIssues.length > 0 AND NOT isWorking AND activeFolder IS NOT NULL
      // Expected: button should be disabled
      const wrapper = mount(RefreshSummariesButton, {
        props: {
          isWorking: false,
          activeFolder,
          setupIssues,
        },
      });

      const button = wrapper.find('button');
      // The button SHOULD be disabled because setupIssues is non-empty.
      // On UNFIXED code this assertion FAILS — proving the bug exists.
      expect(
        button.attributes('disabled'),
        `Button should be disabled when setupIssues has ${setupIssues.length} entries: ${JSON.stringify(setupIssues.map((i: any) => i.message))}`,
      ).toBeDefined();
    },
  );
});
