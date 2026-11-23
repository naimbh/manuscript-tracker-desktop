/** Model constants, copied from the web app's app/Models/*.php. */

export const STAGES = {
  idea: 'Idea',
  drafting: 'Drafting',
  ready: 'Ready to submit',
  submitted: 'Submitted',
  under_review: 'Under review',
  revision: 'Revising',
  accepted: 'Accepted',
  published: 'Published',
  shelved: 'Shelved',
};

export const DECISIONS = {
  submitted: 'Submitted',
  with_editor: 'With editor',
  pending: 'Under review',
  awaiting_decision: 'Awaiting decision',
  returned: 'Returned to author',
  desk_reject: 'Desk rejected',
  reject: 'Rejected',
  major: 'Major revision',
  minor: 'Minor revision',
  accept: 'Accepted',
  withdrawn: 'Withdrawn',
};

/** Statuses where the paper is with the journal and no decision has been made yet. */
export const WAITING = ['submitted', 'with_editor', 'pending', 'awaiting_decision', 'returned'];

/** Submission::stageFor(): the paper stage that matches a submission status. */
export function stageFor(decision) {
  switch (decision) {
    case 'submitted': case 'with_editor': case 'returned': return 'submitted';
    case 'pending': case 'awaiting_decision': return 'under_review';
    case 'major': case 'minor': return 'revision';
    case 'accept': return 'accepted';
    default: return 'ready';
  }
}

/** Submission helpers; `s` is a submission row ({ decision, ... }) or a decision string. */
const dec = (s) => (typeof s === 'string' ? s : s?.decision);
export const isWaiting = (s) => WAITING.includes(dec(s));
export const isActive = (s) => isWaiting(s) || ['major', 'minor'].includes(dec(s));
export const isRejection = (s) => ['desk_reject', 'reject', 'withdrawn'].includes(dec(s));

/** Submission::tone(). */
export function tone(s) {
  switch (dec(s)) {
    case 'accept': return 'green';
    case 'minor': case 'major': return 'amber';
    case 'desk_reject': case 'reject': return 'red';
    case 'withdrawn': return 'zinc';
    case 'submitted': return 'indigo';
    case 'with_editor': return 'fuchsia';
    case 'awaiting_decision': return 'teal';
    case 'returned': return 'orange';
    default: return 'blue';
  }
}

export const VENUE_KINDS = { journal: 'Journal', conference: 'Conference', preprint: 'Preprint server' };
export const REVIEW_TYPES = { double: 'Double-anonymous', single: 'Single-anonymous', open: 'Open review', none: 'No peer review' };
export const PREPRINT_POLICIES = { allowed: 'Preprints allowed', after: 'Only after decision', discouraged: 'Discouraged', unknown: 'Not checked' };

export const PUBLICATION_KINDS = {
  article: 'Journal article', conference: 'Conference paper', letter: 'Letter / short paper',
  chapter: 'Book chapter', thesis: 'Thesis', preprint: 'Preprint',
};

export const CREDIT = [
  'Conceptualization', 'Data curation', 'Formal analysis', 'Funding acquisition', 'Investigation',
  'Methodology', 'Project administration', 'Resources', 'Software', 'Supervision', 'Validation',
  'Visualization', 'Writing – original draft', 'Writing – review & editing',
];

export const FILE_KINDS = {
  manuscript: 'Manuscript',
  anonymized: 'Anonymized manuscript',
  cover: 'Cover letter',
  response: 'Response to reviewers',
  proof: 'Proof PDF',
  supplementary: 'Supplementary',
};

export const OPPORTUNITY_KINDS = { conference: 'Conference', special_issue: 'Special issue', journal: 'Journal call', workshop: 'Workshop' };

/** Namespaced aliases mirroring the PHP classes: Paper.STAGES, Submission.DECISIONS, ... */
export const Paper = { STAGES };
export const Submission = { DECISIONS, WAITING, stageFor, isWaiting, isActive, isRejection, tone };
export const Venue = { KINDS: VENUE_KINDS, REVIEW_TYPES, PREPRINT_POLICIES };
export const Publication = { KINDS: PUBLICATION_KINDS };
export const Author = { CREDIT };
export const PaperFile = { KINDS: FILE_KINDS };
export const Opportunity = { KINDS: OPPORTUNITY_KINDS };
