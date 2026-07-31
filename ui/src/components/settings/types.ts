export type SettingsTab =
  | 'general'
  | 'ai'
  | 'canopy'
  | 'dataforseo'
  | 'storydata'
  | 'archived';

export const SETTINGS_TABS: { id: SettingsTab; label: string }[] = [
  { id: 'general', label: 'General' },
  { id: 'ai', label: 'AI Models' },
  { id: 'canopy', label: 'Canopy' },
  { id: 'dataforseo', label: 'DataForSEO' },
  { id: 'storydata', label: 'Story Data' },
  { id: 'archived', label: 'Archived Reports' },
];
