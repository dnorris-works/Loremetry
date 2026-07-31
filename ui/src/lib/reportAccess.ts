/** Pricing/access badge for a report card (maps model min_tier to subscription surface). */

export type ReportAccessBadge = 'free' | 'subscriber' | 'locked';

export function isSubscriberRole(role: string, isAdmin: boolean): boolean {
  if (isAdmin) return true;
  return role === 'subscriber' || role === 'admin';
}

/**
 * Reports with min_tier capable/strong are subscriber-tier; basic is free-tier.
 * Locked when the report requires subscriber access but the user is not subscribed.
 */
export function reportAccessBadge(
  minTier: string,
  isSubscriber: boolean,
): ReportAccessBadge {
  const tier = (minTier || 'basic').toLowerCase();
  const premium = tier === 'capable' || tier === 'strong';
  if (premium && !isSubscriber) return 'locked';
  if (premium) return 'subscriber';
  return 'free';
}

export function reportAccessLabel(badge: ReportAccessBadge): string {
  switch (badge) {
    case 'free':
      return 'Free';
    case 'subscriber':
      return 'Included';
    case 'locked':
      return 'Locked';
  }
}
