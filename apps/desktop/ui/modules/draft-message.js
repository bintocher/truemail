// truemail UI module: draft-message.js
// Which letter is an unsent draft and which actions apply to it
// (specs/draft-message-actions.md). Pure functions without DOM and without the
// Tauri API: the reading pane, its "more" menu and the list menu apply them.
'use strict';

// Actions that apply to a draft: the rest belongs to an incoming letter.
const DRAFT_ACTIONS = Object.freeze(new Set([
  'send-draft', 'edit-draft', 'trash', 'flag-message', 'task-message', 'flag', 'important-toggle', 'pin-message', 'task-edit',
  'raw', 'export-eml', 'eml', 'delete-forever', 'settings',
]));

// A draft is known by its flag or by its folder: some transports do not set the flag.
function isDraftMessage(message, folders) {
  if (!message) return false;
  if (message.flags?.draft) return true;
  return (folders || []).some(folder => folder.id === message.folder_id && folder.role === 'drafts');
}

// The actions that apply to the given letter, in their own order.
function actionsForMessage(message, folders, actions) {
  if (!isDraftMessage(message, folders)) return [...(actions || [])];
  return (actions || []).filter(action => DRAFT_ACTIONS.has(typeof action === 'string' ? action : action.key));
}

const draftMessageModel = {DRAFT_ACTIONS, isDraftMessage, actionsForMessage};
if (typeof window !== 'undefined') window.draftMessageModel = draftMessageModel;
if (typeof module !== 'undefined' && module.exports) module.exports = draftMessageModel;
