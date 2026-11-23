// A draft in the reading pane, in its "more" menu and in the list menu, and its
// way out through the sending queue: the real markup, the real handlers and the
// bridge. Before, a draft offered the actions of an incoming letter and could
// not be sent from the program at all.
// Specification: specs/draft-message-actions.md.
// Run: node --test apps/desktop/tests/js/draft-message.test.js (Node 22+).

'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const {createUiWindow} = require('./ui-window.js');
const draftMessageModel = require('../../ui/modules/draft-message.js');

const ACCOUNTS = [{id: 1, email: 'me@example.test', display_name: 'Me'}, {id: 2, email: 'work@example.test', display_name: 'Work'}];
const FOLDERS = [
  {id: 2, account_id: 1, role: 'inbox', display_name: 'Входящие', remote_path: 'INBOX'},
  {id: 3, account_id: 1, role: 'drafts', display_name: 'Черновики', remote_path: 'Drafts'},
];
const INCOMING = {id: 11, account_id: 1, folder_id: 2, subject: 'Входящее', preview: '',
  from: {name: 'Boss', email: 'boss@example.test'}, to: [{name: '', email: 'me@example.test'}], cc: [],
  flags: {seen: true, flagged: false, draft: false}, labels: [], date: '2026-11-23T10:00:00Z', pinned_at: null};
const DRAFT = {id: 21, account_id: 2, folder_id: 3, subject: 'Не отправлено', preview: '',
  from: {name: 'Me', email: 'me@example.test'}, to: [{name: 'Mate', email: 'mate@example.test'}], cc: [],
  flags: {seen: true, flagged: false, draft: true}, labels: [], date: '2026-11-23T09:00:00Z', pinned_at: null};
const INCOMING2 = {...INCOMING, id: 12, subject: 'Тема B', from: {name: 'Bob', email: 'bob@example.test'}, date: '2026-11-23T11:00:00Z'};
// No draft flag and no recipient: the folder alone tells that it is a draft.
const UNADDRESSED = {...DRAFT, id: 22, to: [], flags: {seen: true, flagged: false, draft: false}};
const INCOMING3 = {...INCOMING, id: 13, subject: 'Тема C', date: '2026-11-23T08:00:00Z'};
const LETTERS = {11: INCOMING, 12: INCOMING2, 13: INCOMING3, 21: DRAFT, 22: UNADDRESSED};

const fullOf = id => ({
  meta: LETTERS[id],
  body_html: id === 12 ? '<p>Текст B</p>' : '<p>Текст письма</p>', body_text: id === 12 ? 'Текст B' : 'Текст письма', attachments: [],
  has_remote_content: false, is_newsletter: false, unsubscribe: null, body_fetched: true,
  bcc: [{name: 'Hidden', email: 'hidden@example.test'}],
});
// A common attachment of a letter.
const part = (id, extra = {}) => ({id, filename: 'a.pdf', mime_type: 'application/pdf', size: 3, is_inline: false, content_id: null, fetched: true, embedded: false, ...extra});
const CONTENT = {filename: 'a.pdf', mime_type: 'application/pdf', base64: 'QQ=='};

const settle = async () => {
  for (let round = 0; round < 12; round += 1) await new Promise(resolve => setImmediate(resolve));
};
const shown = node => Boolean(node) && !node.classList.contains('hidden') && !node.classList.contains('toolbar-original-hidden');
const paneActions = ui => ui.queryAll('.thead [data-act]').filter(shown).map(node => node.dataset.act);
const menuActions = ui => ui.queryAll('#ctxmenu [data-context-action]').map(node => node.dataset.contextAction);
// Messages for the user are entries of the event journal in the status line.
const notices = ui => ui.queryAll('#activityPanel .activity-entry-text').map(node => node.textContent);
const attachmentNames = ui => ui.evaluate('composerAttachments.map(item=>item.filename)');

async function openMail(answers = {}) {
  const ui = createUiWindow({
    answers: {
      getMessage: id => fullOf(id),
      sendMessage: () => ({account_id: 1, operation_id: 501, cancel_until: null}),
      messageAction: ids => ({operation_ids: ids.map(id => id + 1000), skipped: 0}),
      listQuickSteps: () => [],
      ...answers,
    },
    globals: {reloadCoreData: async () => {}},
  });
  await ui.clock.drain();
  await ui.evaluate('window.localizationReady');
  ui.evaluate(`
    coreAccounts=${JSON.stringify(ACCOUNTS)};
    setCoreFolders(${JSON.stringify(FOLDERS)});
    currentFolderId=3;currentSmartIndex=null;
    messages=${JSON.stringify(Object.values(LETTERS))};
    applyListOptions(true,'Черновики');
  `);
  return ui;
}

const openLetter = async (ui, id) => {
  ui.evaluate(`showMessage(messages.find(message=>message.id===${id}))`);
  await settle();
};
const sent = ui => ui.callsOf('sendMessage').map(call => call.args[0]);
const trashed = ui => ui.callsOf('messageAction').map(call => [[...call.args[0]], call.args[1]]);
const openListMenu = (ui, id) => {
  ui.query(`#msgs .msg[data-message-id="${id}"]`).dispatch('contextmenu', {clientX: 20, clientY: 20});
  return menuActions(ui);
};

test('S-001: a draft is known by its flag or by its folder and keeps only the actions of a draft', () => {
  assert.equal(draftMessageModel.isDraftMessage(DRAFT, FOLDERS), true);
  assert.equal(draftMessageModel.isDraftMessage(UNADDRESSED, FOLDERS), true, 'a draft without the flag is not recognised by its folder');
  assert.equal(draftMessageModel.isDraftMessage(INCOMING, FOLDERS), false);
  assert.equal(draftMessageModel.isDraftMessage(null, FOLDERS), false);
  const actions = ['reply', 'trash', 'archive', 'quick-step:3', 'raw'].map(key => ({key}));
  assert.deepEqual(draftMessageModel.actionsForMessage(DRAFT, FOLDERS, actions).map(action => action.key), ['trash', 'raw']);
  assert.equal(draftMessageModel.actionsForMessage(INCOMING, FOLDERS, actions).length, 5, 'an ordinary letter lost an action');
});

test('S-002, S-003, S-004, S-009: the pane and both menus follow the letter, an ordinary letter keeps its actions', async () => {
  const ui = await openMail();

  await openLetter(ui, 21);
  assert.ok(paneActions(ui).includes('send-draft'), 'the draft has no Send button');
  for (const action of ['reply', 'replyall', 'forward', 'archive']) {
    assert.equal(paneActions(ui).includes(action), false, `the draft offers "${action}"`);
  }
  assert.ok(paneActions(ui).includes('trash'), 'the draft cannot be deleted');
  assert.equal(shown(ui.query('#threadMoreMenu [data-thread-action="create-rule"]')), false, 'the draft offers a rule');
  assert.equal(shown(ui.query('#threadMoreMenu [data-thread-action="block-sender"]')), false, 'the draft offers blocking its sender');
  assert.equal(shown(ui.query('#threadMoreMenu [data-thread-action="raw"]')), true, 'the draft lost its source view');
  assert.equal(ui.queryAll('#threadMoreMenu .hidden').length, 6, 'the "more" menu of a draft hides another set of items');

  // An ordinary letter keeps every action it had.
  await openLetter(ui, 11);
  assert.equal(paneActions(ui).includes('send-draft'), false, 'an incoming letter offers Send');
  for (const action of ['reply', 'replyall', 'forward', 'archive']) {
    assert.equal(paneActions(ui).includes(action), true, `an incoming letter lost "${action}"`);
  }
  assert.equal(shown(ui.query('#threadMoreMenu [data-thread-action="create-rule"]')), true);
  assert.equal(ui.queryAll('#threadMoreMenu .hidden').length, 0, 'an incoming letter lost an item of the "more" menu');

  const draftMenu = openListMenu(ui, 22);
  assert.equal(draftMenu[0], 'send-draft', 'the list menu of a draft does not start with Send');
  for (const action of ['reply', 'archive', 'create-rule', 'block-sender', 'ignore-conversation', 'sweep-sender']) {
    assert.equal(draftMenu.includes(action), false, `the list menu of a draft offers "${action}"`);
  }
  assert.equal(draftMenu.includes('trash'), true, 'the list menu of a draft cannot delete it');
  assert.equal(draftMenu.includes('raw'), true, 'the list menu of a draft lost its source view');

  // The list menu acts on the letter under the cursor; the buttons above the
  // text stay with the letter that is shown there.
  assert.equal(paneActions(ui).includes('send-draft'), false, 'Send appeared above an ordinary letter after the menu of a draft');
  assert.equal(paneActions(ui).includes('reply'), true, 'the shown ordinary letter lost Reply after the menu of a draft');

  ui.evaluate("currentFolderId=2;applyListOptions(true,'Входящие')");
  const incomingMenu = openListMenu(ui, 11);
  assert.equal(incomingMenu.includes('send-draft'), false, 'an incoming letter offers Send in its menu');
  assert.equal(incomingMenu.includes('reply'), true, 'an incoming letter lost Reply in its menu');
  assert.equal(incomingMenu.includes('create-rule'), true, 'an incoming letter lost its rule action');

  // Once the open draft is gone, the pane is the pane of any other letter.
  ui.evaluate("currentFolderId=3;applyListOptions(true,'Черновики')");
  await openLetter(ui, 22);
  assert.equal(paneActions(ui).includes('send-draft'), true);
  ui.evaluate("performMessageAction('trash')");
  await settle();
  // The next letter of the list takes the place of the deleted one, and the pane is its pane now.
  assert.equal(ui.evaluate('shownMessageId'), 21, 'the neighbour of the deleted draft is not shown');
  ui.evaluate("performMessageAction('trash')");
  await settle();
  assert.equal(paneActions(ui).includes('send-draft'), false, 'Send stays on the pane after the drafts were deleted');
});

test('S-002, S-005, S-007: the button above the text sends the letter below it, the editor stays closed', async () => {
  const ui = await openMail();
  await openLetter(ui, 21);
  // The list menu makes another draft active; the button above the shown text
  // still sends the shown letter.
  openListMenu(ui, 22);
  assert.equal(ui.evaluate('activeMessage.id'), 22);
  assert.equal(paneActions(ui).includes('send-draft'), true, 'Send left the shown draft after the menu of another letter');

  ui.byId('threadSendDraft').onclick();
  await settle();
  const request = sent(ui)[0];
  assert.equal(request.account_id, 2, 'the letter went out from another account');
  assert.deepEqual([...request.to], ['Mate <mate@example.test>']);
  assert.deepEqual([...request.bcc], ['Hidden <hidden@example.test>'], 'the hidden copy of the draft was lost');
  assert.equal(request.subject, 'Не отправлено');
  assert.equal(request.body_html, '<p>Текст письма</p>');
  assert.equal(request.body_text, 'Текст письма', 'the text part of the letter was rebuilt from the markup');
  assert.deepEqual(trashed(ui), [[[21], 'trash']], 'the sent draft stayed in Drafts');
  assert.equal(paneActions(ui).includes('send-draft'), false, 'Send stays on the pane after the draft went out');
  assert.equal(ui.byId('composeView').classes.has('active'), false, 'the editor opened for a ready draft');
  assert.equal(ui.evaluate('composerHasContent()'), false, 'the editor was touched by the sending of a ready draft');
});

test('S-002, S-003: the buttons above the text act on the shown draft after the list menu of another letter', async () => {
  const ui = await openMail();
  await openLetter(ui, 21);
  // The menu of another draft is closed without a choice.
  openListMenu(ui, 22);
  ui.document.dispatch('click', {target: ui.document.body});
  assert.equal(ui.evaluate('activeMessage.id'), 21, 'the letter of the closed menu stays active');

  // An item of the menu still belongs to the letter under the cursor, and the
  // shown letter is active again once the item has run.
  openListMenu(ui, 22);
  ui.query('#ctxmenu [data-context-action="trash"]').dispatch('click');
  await settle();
  assert.deepEqual(trashed(ui), [[[22], 'trash']], 'the item of the menu acted on the shown letter');
  assert.equal(ui.evaluate('paneLetter()?.id'), 21, 'the menu of another letter took the shown draft off the screen');
  assert.equal(paneActions(ui).includes('send-draft'), true, 'Send left the shown draft after the menu of another letter');
  openListMenu(ui, 22);
  ui.query('#ctxmenu [data-context-action="raw"]').dispatch('click');
  assert.equal(ui.evaluate('activeMessage.id'), 21, 'the letter of the menu stays active after its item has run');

  // A click reaches the button before the document closes the menu.
  openListMenu(ui, 22);
  ui.query('.thead [data-act="trash"]').dispatch('click');
  await settle();
  assert.deepEqual(trashed(ui).at(-1), [[21], 'trash'], 'the button above the shown draft acted on the letter of the list menu');
});

test('S-002: the "more" menu of the pane acts on the shown draft after the list menu of another letter', async () => {
  const ui = await openMail({messageRaw: () => 'raw letter'});
  await openLetter(ui, 21);
  openListMenu(ui, 22);
  ui.byId('threadMoreButton').dispatch('click');
  assert.equal(ui.evaluate('activeMessage.id'), 21, 'the "more" menu of the pane took the letter of the list menu');
  ui.query('#threadMoreMenu [data-thread-action="raw"]').dispatch('click');
  await settle();
  assert.equal(ui.callsOf('messageRaw').at(-1).args[0], 21, 'the source of another letter was opened');
});

test('S-002: an empty reading pane offers nothing to send', async () => {
  const ui = await openMail();
  await openLetter(ui, 21);
  // The folder changes: the shown draft is not in the list, and the pane is cleared.
  ui.evaluate("currentFolderId=2;applyListOptions(true,'Входящие')");
  assert.equal(ui.evaluate('paneLetter()'), null, 'the cleared pane still names a letter');
  assert.equal(paneActions(ui).includes('send-draft'), false, 'Send stays above an empty pane');
  ui.evaluate('sendDraftMessage(paneLetter())');
  await settle();
  assert.equal(sent(ui).length, 0, 'a letter went out from an empty pane');
});

test('S-017: the next draft gets its Send button after a draft that is already on its way', async () => {
  const ui = await openMail({messageAction: () => ({operation_ids: [], skipped: 1, skipped_no_folder: 1})});
  await openLetter(ui, 21);
  ui.byId('threadSendDraft').onclick();
  await settle();
  // The refresh brings the sent draft back, and the user opens another draft.
  ui.evaluate(`messages=${JSON.stringify(Object.values(LETTERS))};showMessage(messages.find(message=>message.id===22))`);
  await settle();
  assert.equal(paneActions(ui).includes('send-draft'), true, 'the next draft lost Send behind a draft that is on its way');
  ui.evaluate('showMessage(messages.find(message=>message.id===21))');
  await settle();
  assert.equal(paneActions(ui).includes('send-draft'), false, 'Send came back for the draft whose letter is on its way');
});

test('S-005, S-006: a draft without a recipient opens the editor and goes out from it', async () => {
  const ui = await openMail();
  await openLetter(ui, 22);
  ui.byId('threadSendDraft').onclick();
  await settle();
  assert.equal(ui.byId('composeView').classes.has('active'), true, 'the draft without a recipient did not open the editor');
  assert.equal(sent(ui).length, 0, 'a draft without a recipient was sent');
  assert.equal(ui.byId('compSubj').value, 'Не отправлено');
  assert.match(ui.byId('composeStatus').textContent, /получател/, 'the editor does not say what is missing');
  assert.equal(trashed(ui).length, 0, 'an unsent draft was removed from Drafts');

  ui.evaluate("setRecipients('compTo',['mate@example.test'])");
  await ui.byId('compSend').onclick();
  await settle();
  assert.equal(sent(ui).length, 1, 'the draft was not sent from the editor');
  assert.deepEqual(trashed(ui), [[[22], 'trash']], 'the draft sent from the editor stayed in Drafts');
  assert.equal(ui.byId('composeView').classes.has('active'), false);
});

test('S-005, S-006: a letter scheduled from the editor also takes its draft out of the folder', async () => {
  const ui = await openMail({scheduleMessage: () => 77});
  await openLetter(ui, 22);
  ui.byId('threadSendDraft').onclick();
  await settle();
  ui.evaluate("setRecipients('compTo',['mate@example.test']);const at=document.getElementById('compSendAt');at.classList.remove('hidden');at.value='2030-01-01T09:00';");
  await ui.byId('compSendLater').onclick();
  await settle();
  assert.equal(ui.callsOf('scheduleMessage').length, 1, 'the letter was not scheduled');
  assert.deepEqual(trashed(ui), [[[22], 'trash']], 'the scheduled letter left its draft in Drafts');
});

test('S-012, S-019: a letter scheduled from the editor is queued once and does not touch another letter', async () => {
  let answer = null;
  const ui = await openMail({scheduleMessage: () => new Promise(resolve => { answer = resolve; })});
  await openLetter(ui, 22);
  ui.byId('threadSendDraft').onclick();
  await settle();
  ui.evaluate("setRecipients('compTo',['mate@example.test']);const at=document.getElementById('compSendAt');at.classList.remove('hidden');at.value='2030-01-01T09:00';");
  const scheduling = ui.byId('compSendLater').onclick();
  await ui.byId('compSendLater').onclick();
  await settle();
  assert.equal(ui.callsOf('scheduleMessage').length, 1, 'a second click queued the letter twice');
  // Another letter takes the editor while the queue answers.
  ui.evaluate("document.getElementById('composeBtn').onclick()");
  await settle();
  ui.evaluate("document.getElementById('compSubj').value='Другое письмо'");
  answer(77);
  await scheduling;
  await settle();
  assert.equal(ui.byId('compSubj').value, 'Другое письмо', 'the answer of the queue wiped another letter');
  assert.equal(ui.byId('composeView').classes.has('active'), true, 'the user was pulled away from the letter being written');
  assert.deepEqual(trashed(ui), [[[22], 'trash']], 'the scheduled letter left its draft in Drafts');
});

test('S-031: the sender chosen in the editor survives a refresh of the account list', async () => {
  const ui = await openMail();
  ui.evaluate("resetComposer();const from=document.querySelector('.from-sel');from.value='2';window.rememberComposerFrom();");
  // Every refresh of the data clears the list of the accounts and draws it again.
  ui.evaluate('clearDemoData();renderAccountSettings(coreAccounts,[],[])');
  assert.equal(ui.evaluate("document.querySelector('.from-sel').value"), '2', 'the refresh returned the sender to the first account');
  // The same holds for the letter that comes back from the saved state at start.
  ui.evaluate("resetComposer();window.pendingComposerDraft={account_id:2,to:[],cc:[],bcc:[],subject:'',body_html:'',attachments:[]};renderAccountSettings(coreAccounts,[],[]);clearDemoData();renderAccountSettings(coreAccounts,[],[])");
  assert.equal(ui.evaluate("document.querySelector('.from-sel').value"), '2', 'the first refresh returned the restored letter to the first account');
});

test('S-007, S-008: another unsent letter in the editor is not touched by a draft that has to be finished', async () => {
  const ui = await openMail();
  await openLetter(ui, 22);
  ui.evaluate("resetComposer();setRecipients('compTo',['other@example.test']);document.getElementById('compSubj').value='Другое письмо';");
  ui.byId('threadSendDraft').onclick();
  await settle();
  assert.equal(sent(ui).length, 0, 'the unfinished draft was sent over another letter');
  assert.equal(ui.byId('compSubj').value, 'Другое письмо', 'the draft replaced the letter being written');
  assert.equal(ui.byId('composeView').classes.has('active'), true, 'the user was not brought to the letter being written');
  assert.equal(trashed(ui).length, 0);
});

test('S-008: a draft left in the editor is offered again without a false warning', async () => {
  const ui = await openMail();
  await openLetter(ui, 22);
  ui.byId('threadSendDraft').onclick();
  await settle();
  ui.evaluate("showView('mailView')");
  ui.byId('threadSendDraft').onclick();
  await settle();
  assert.equal(ui.byId('composeView').classes.has('active'), true, 'the editor with the same draft did not open');
  assert.equal(notices(ui).some(text => /другое неотправленное/.test(text)), false, 'the same draft is reported as another letter');
});

test('S-011: a second click on Send while the draft is being read does not queue the letter twice', async () => {
  const readers = [];
  let reads = 0;
  const ui = await openMail({getMessage: id => {
    reads += 1;
    // The letter is shown at once; the sending reads are released by the test.
    if (reads === 1) return fullOf(id);
    return new Promise(resolve => { readers.push(() => resolve(fullOf(id))); });
  }});
  await openLetter(ui, 21);
  ui.byId('threadSendDraft').onclick();
  ui.byId('threadSendDraft').onclick();
  await settle();
  assert.equal(readers.length, 1, 'the second click started its own reading of the draft');
  readers[0]();
  await settle();
  assert.equal(sent(ui).length, 1, 'the letter was queued twice');
  assert.deepEqual(trashed(ui), [[[21], 'trash']]);
});

test('S-011: the draft leaves the screen as soon as the letter is queued', async () => {
  let release = null;
  const ui = await openMail({messageAction: () => new Promise(resolve => { release = resolve; })});
  await openLetter(ui, 21);
  ui.byId('threadSendDraft').onclick();
  await settle();
  assert.equal(sent(ui).length, 1);
  assert.equal(paneActions(ui).includes('send-draft'), false, 'Send is still offered while the letter is already queued');
  ui.byId('threadSendDraft').onclick();
  await settle();
  assert.equal(sent(ui).length, 1, 'a draft that was already sent was sent again');
  release({operation_ids: [1021], skipped: 0});
  await settle();
  assert.equal(trashed(ui).length, 1);
});

test('S-011, S-007: a draft that stayed in its folder is reported and not sent a second time', async () => {
  const ui = await openMail({messageAction: () => ({operation_ids: [], skipped: 1, skipped_no_folder: 1})});
  await openLetter(ui, 21);
  ui.byId('threadSendDraft').onclick();
  await settle();
  assert.equal(sent(ui).length, 1, 'the letter must go out even when the draft cannot be removed');
  assert.ok(notices(ui).some(text => /вручную/.test(text)), 'the draft left in its folder is not reported');
  assert.equal(ui.callsOf('uiLog').length, 1, 'the failure is not written to the program log');
  // The refresh brings the draft that looks unsent back to the list.
  ui.evaluate(`messages=${JSON.stringify(Object.values(LETTERS))};showMessage(messages.find(message=>message.id===21))`);
  await settle();
  assert.equal(paneActions(ui).includes('send-draft'), false, 'Send is offered again for a draft whose letter is already on its way');
  ui.evaluate('sendDraftMessage()');
  await settle();
  assert.equal(sent(ui).length, 1, 'the same draft was queued a second time');
});

test('S-007: a draft already in the Trash is not reported as a failure', async () => {
  const ui = await openMail({messageAction: () => ({operation_ids: [], skipped: 0})});
  await openLetter(ui, 21);
  ui.byId('threadSendDraft').onclick();
  await settle();
  assert.equal(sent(ui).length, 1);
  assert.equal(notices(ui).some(text => /вручную/.test(text)), false, 'a draft that needed no move is reported as a failure');
  assert.equal(ui.callsOf('uiLog').length, 0);
});

test('S-012: the editor is not touched while a draft that has to be finished is being read', async () => {
  const readers = [];
  const ui = await openMail({
    getMessage: id => ({...fullOf(id), attachments: [part(0)]}),
    attachmentContent: () => new Promise(resolve => { readers.push(() => resolve(CONTENT)); }),
  });
  await openLetter(ui, 22);
  ui.byId('threadSendDraft').onclick();
  await settle();
  assert.equal(readers.length, 1, 'the attachment of the draft is not being read');
  ui.evaluate("document.getElementById('composeBtn').onclick()");
  await settle();
  ui.evaluate("setRecipients('compTo',['other@example.test']);document.getElementById('compSubj').value='Моё письмо'");
  readers[0]();
  await settle();
  assert.equal(ui.byId('compSubj').value, 'Моё письмо', 'the draft replaced the letter being written');
  assert.equal(ui.evaluate('recipientModel.compTo.length'), 1, 'the draft removed the recipient of another letter');
  assert.deepEqual([...attachmentNames(ui)], [], 'an attachment of the draft landed in another letter');
  assert.equal(sent(ui).length, 0, 'the draft was sent over another letter');
  assert.equal(trashed(ui).length, 0);
  assert.ok(notices(ui).some(text => /другое неотправленное/.test(text)), 'the user is not told why the draft stayed unsent');
});

test('S-012: a letter started while the queue answers stays, and the draft is still taken out', async () => {
  let answer = null;
  const ui = await openMail({sendMessage: () => new Promise(resolve => { answer = resolve; })});
  await openLetter(ui, 21);
  ui.byId('threadSendDraft').onclick();
  await settle();
  assert.equal(typeof answer, 'function', 'the letter was not offered to the queue');
  ui.evaluate("document.getElementById('composeBtn').onclick()");
  await settle();
  ui.evaluate("setRecipients('compTo',['other@example.test']);document.getElementById('compSubj').value='Моё письмо'");
  answer({account_id: 1, operation_id: 501, cancel_until: null});
  await settle();
  assert.equal(ui.byId('compSubj').value, 'Моё письмо', 'the answer of the queue wiped another letter');
  assert.equal(ui.byId('composeView').classes.has('active'), true, 'the user was pulled away from the letter being written');
  assert.deepEqual(trashed(ui), [[[21], 'trash']], 'the queued draft was not taken out of its folder');
});

test('S-012: a letter started in the editor while the editor sends is not wiped by the answer', async () => {
  let answer = null;
  const ui = await openMail({sendMessage: () => new Promise(resolve => { answer = resolve; })});
  await openLetter(ui, 22);
  ui.byId('threadSendDraft').onclick();
  await settle();
  ui.evaluate("setRecipients('compTo',['mate@example.test'])");
  const sending = ui.byId('compSend').onclick();
  await settle();
  assert.equal(typeof answer, 'function', 'the letter was not offered to the queue');
  ui.evaluate("document.getElementById('composeBtn').onclick()");
  await settle();
  ui.evaluate("document.getElementById('compSubj').value='Моё письмо'");
  answer({account_id: 1, operation_id: 501, cancel_until: null});
  await sending;
  await settle();
  assert.equal(ui.byId('compSubj').value, 'Моё письмо', 'the answer of the queue wiped another letter');
  assert.deepEqual(trashed(ui), [[[22], 'trash']], 'the queued draft was not taken out of its folder');
});

test('S-012: a file handed over from the file manager while the draft is being read stays out of the letter', async () => {
  const readers = [];
  const ui = await openMail({
    getMessage: id => ({...fullOf(id), attachments: [part(0)]}),
    attachmentContent: () => new Promise(resolve => { readers.push(() => resolve(CONTENT)); }),
    readLocalFile: async () => ({filename: 'local.txt', mime_type: 'text/plain', base64: 'bG9jYWw='}),
  });
  await openLetter(ui, 21);
  ui.byId('threadSendDraft').onclick();
  await settle();
  ui.evaluate("attachFilesToComposer(['C:/tmp/local.txt'])");
  await settle();
  readers[0]();
  await settle();
  assert.equal(sent(ui).length, 1, 'the draft was not sent');
  assert.deepEqual([...sent(ui)[0].attachments].map(item => item.filename), ['a.pdf'], 'a file of the file manager joined the letter');
  assert.deepEqual([...attachmentNames(ui)], ['local.txt'], 'the file of the file manager was lost');
  assert.equal(ui.byId('composeView').classes.has('active'), true, 'the editor with the file of the file manager was closed');
});

test('S-013: a draft stored as headers only is not sent as its preview', async () => {
  const ui = await openMail({getMessage: id => ({...fullOf(id), body_fetched: false, body_text: 'превью'})});
  await openLetter(ui, 21);
  ui.byId('threadSendDraft').onclick();
  await settle();
  assert.equal(sent(ui).length, 0, 'the preview of a draft was sent as its body');
  assert.equal(trashed(ui).length, 0, 'a draft that was not sent was removed');
  assert.ok(notices(ui).some(text => /загружено/.test(text)), 'the user is not told why the draft was not sent');
});

test('S-014, S-015: attachments go along once, a name with a comma is one address', async () => {
  const ui = await openMail({
    getMessage: id => ({
      ...fullOf(id),
      meta: {...LETTERS[id], to: [{name: 'Ivanov, Petr', email: 'p@example.test'}, {name: 'Иванов "Пётр"', email: 'q@example.test'}]},
      attachments: [
        part(0),
        part(1, {filename: 'pic.png', mime_type: 'image/png', is_inline: true, content_id: 'pic@example.test', embedded: true}),
        part(2, {filename: 'report.pdf', is_inline: true, content_id: 'report@example.test'}),
      ],
    }),
    attachmentContent: () => CONTENT,
  });
  await openLetter(ui, 21);
  ui.byId('threadSendDraft').onclick();
  await settle();
  const request = sent(ui)[0];
  assert.deepEqual([...request.to], ['"Ivanov, Petr" <p@example.test>', '"Иванов \\"Пётр\\"" <q@example.test>'], 'a name with a comma or a quote is not quoted as one address');
  assert.deepEqual([...request.attachments].map(item => item.filename), ['a.pdf', 'a.pdf'], 'an embedded picture was attached again or a file with an identifier was dropped');
  assert.equal(ui.callsOf('attachmentContent').length, 2);
});

test('S-024: a name with quotes is not escaped a second time on the way back', async () => {
  const ui = await openMail();
  const address = '"Иванов \\"Пётр\\"" <p@example.test>';
  ui.evaluate(`resetComposer();setRecipients('compTo',[${JSON.stringify(address)}])`);
  assert.deepEqual([...ui.evaluate('composerRequest()').to], [address], 'the quotes of the name were escaped a second time');
});

test('S-010: a letter of markup alone does not get a text part written for it', async () => {
  const ui = await openMail({getMessage: id => ({...fullOf(id), has_text_part: false})});
  await openLetter(ui, 21);
  ui.byId('threadSendDraft').onclick();
  await settle();
  assert.equal(sent(ui)[0].body_text, '', 'the text of the letter was made from its markup');
});

test('S-012: a letter the core refuses opens the editor with the reason', async () => {
  const ui = await openMail({sendMessage: () => Promise.reject({kind: 'account_config', message: 'некорректный адрес "long@example.test"'})});
  await openLetter(ui, 21);
  ui.byId('threadSendDraft').onclick();
  await settle();
  assert.equal(ui.byId('composeView').classes.has('active'), true, 'the refused letter did not open in the editor');
  assert.equal(ui.byId('compSubj').value, 'Не отправлено', 'the refused letter is not in the editor');
  assert.match(ui.byId('composeStatus').textContent, /некорректный адрес/, 'the editor does not say why the core refused the letter');
  assert.equal(trashed(ui).length, 0, 'the refused draft was removed from Drafts');
});

test('S-022: a size the core already knows is checked before the file is read', async () => {
  const ui = await openMail({
    getMessage: id => ({...fullOf(id), attachments: [part(0), part(1, {size: 30 * 1024 * 1024})]}),
    attachmentContent: () => CONTENT,
  });
  await openLetter(ui, 21);
  ui.byId('threadSendDraft').onclick();
  await settle();
  assert.equal(ui.callsOf('attachmentContent').length, 1, 'the file that does not fit was read');
  assert.equal(ui.byId('composeView').classes.has('active'), true, 'the editor did not open for an oversized letter');
  assert.match(ui.byId('composeStatus').textContent, /помещается/, 'the user is not told why the letter was not sent');
});

test('S-022: a letter of pictures alone that is too large opens the editor', async () => {
  // Four different pictures: the same picture is counted once, as the core sends it.
  const pictures = [0, 1, 2, 3].map(index => `<img src="data:image/png;base64,${'A'.repeat(9 * 1024 * 1024 + index * 4)}">`).join('');
  const ui = await openMail({getMessage: id => ({...fullOf(id), body_html: `<p>${pictures}</p>`})});
  await openLetter(ui, 21);
  ui.byId('threadSendDraft').onclick();
  await settle();
  assert.equal(sent(ui).length, 0, 'an oversized letter was queued');
  assert.equal(ui.byId('composeView').classes.has('active'), true, 'the editor did not open for an oversized letter');
  assert.match(ui.byId('composeStatus').textContent, /встроенные картинки/, 'the user is not told why the letter was not sent');
});

test('S-014: a letter above the size limit opens the editor instead of going to the queue', async () => {
  const big = 'A'.repeat(9 * 1024 * 1024);
  const ui = await openMail({
    getMessage: id => ({...fullOf(id), attachments: [part(0), part(1), part(2), part(3)]}),
    attachmentContent: (messageId, attachmentId) => ({filename: `a${attachmentId}.pdf`, mime_type: 'application/pdf', base64: big}),
  });
  await openLetter(ui, 21);
  ui.byId('threadSendDraft').onclick();
  await settle();
  assert.equal(sent(ui).length, 0, 'an oversized letter was queued');
  assert.equal(trashed(ui).length, 0, 'the draft of an unsent letter was removed');
  // The attachment that does not fit is read, measured and left out: the next ones are not read.
  assert.equal(ui.callsOf('attachmentContent').length, 4, 'the attachments were read on after the limit was exceeded');
  assert.equal(ui.byId('composeView').classes.has('active'), true, 'the editor did not open for an oversized letter');
  assert.deepEqual([...attachmentNames(ui)], ['a0.pdf', 'a1.pdf', 'a2.pdf'], 'the attachments that fit were lost');
  assert.match(ui.byId('composeStatus').textContent, /помещается/, 'the user is not told why the letter was not sent');
});

test('S-012: the editor takes the body of the draft without scripts, handlers and outside pictures', async () => {
  const ui = await openMail({getMessage: id => ({
    ...fullOf(id),
    body_html: '<p onclick="x()">Текст</p><script>x()</script><style>body{display:none}</style>'
      + '<img src="https://tracker.example.test/pixel.png" onerror="x()"><img src="data:image/png;base64,QQ==">'
      + '<a href="javascript:x()">ссылка</a><a href="https://example.test">сайт</a>',
  })});
  await openLetter(ui, 22);
  ui.byId('threadSendDraft').onclick();
  await settle();
  const html = ui.byId('compEdit').innerHTML;
  assert.equal(/<script|<style|onclick|onerror|javascript:|tracker\.example/.test(html), false, `the editor took a dangerous piece of the body: ${html}`);
  assert.match(html, /data:image\/png;base64,QQ==/, 'the embedded picture was lost from the editor');
  assert.match(html, /https:\/\/example\.test/, 'a plain link was lost from the editor');
});

test('S-006: when an attachment cannot be read, the open editor keeps the attachments that were read', async () => {
  const ui = await openMail({
    getMessage: id => ({...fullOf(id), attachments: [part(0), part(1, {filename: 'b.pdf'})]}),
    attachmentContent: (messageId, attachmentId) => (attachmentId === 1 ? Promise.reject(new Error('вложение без содержимого')) : CONTENT),
  });
  await openLetter(ui, 21);
  ui.byId('threadSendDraft').onclick();
  await settle();
  assert.equal(ui.byId('composeView').classes.has('active'), true, 'the editor did not open after the attachment failed');
  assert.deepEqual([...attachmentNames(ui)], ['a.pdf'], 'the attachment that was read was lost');
  assert.equal(sent(ui).length, 0);
  assert.equal(trashed(ui).length, 0, 'the draft was removed before its letter went out');
  await ui.byId('compSend').onclick();
  await settle();
  assert.equal(sent(ui).length, 1);
  assert.deepEqual(trashed(ui), [[[21], 'trash']], 'the draft sent from the editor stayed in Drafts');
});

test('S-004, S-009: a late refresh of quick steps does not put their items into the open menu of a draft', async () => {
  const ui = await openMail({listQuickSteps: () => [{id: 5, name: 'Шаг', icon: 'star', sort_order: 0, hotkey_slot: null, actions: [{kind: 'mark_read'}]}]});
  const menu = openListMenu(ui, 22);
  assert.equal(menu.includes('quick-step:5'), false);
  ui.evaluate('reloadQuickSteps()');
  await settle();
  assert.equal(menuActions(ui).includes('quick-step:5'), false, 'a quick step got into the open menu of a draft');
});

test('S-029: a saved letter returns to the editor cleaned', async () => {
  const ui = await openMail();
  ui.evaluate(`resetComposer();window.pendingComposerDraft={account_id:1,to:[],cc:[],bcc:[],subject:'',body_html:'<p onclick="x()" style="position:fixed;top:0">Текст</p><img src="https://tracker.example.test/p.png">',attachments:[]};renderAccountSettings(coreAccounts,[],[])`);
  const html = ui.byId('compEdit').innerHTML;
  assert.equal(/onclick|tracker\.example|position/.test(html), false, `the saved letter came back to the editor uncleaned: ${html}`);
  assert.match(html, /Текст/, 'the text of the saved letter was lost');
});

test('S-025: an address with a comma in its name survives the saved state of the editor', async () => {
  const ui = await openMail();
  ui.evaluate("resetComposer();setRecipients('compTo',[{name:'Ivanov, Petr',email:'p@example.test'}]);document.getElementById('compSubj').value='Письмо';");
  await ui.evaluate('saveDraftNow()');
  const saved = ui.callsOf('setSetting').at(-1).args[1];
  assert.deepEqual(JSON.parse(saved).to, [{name: 'Ivanov, Petr', email: 'p@example.test'}], 'the saved state does not keep the address as it was entered');
  // The next start restores the saved state while the accounts are drawn.
  // At start the saved text is parsed before the accounts are drawn (settings.js).
  ui.evaluate(`resetComposer();window.pendingComposerDraft=JSON.parse(${JSON.stringify(saved)});renderAccountSettings(coreAccounts,[],[])`);
  assert.equal(ui.evaluate('recipientModel.compTo.length'), 1, 'the name was cut into two addresses by the restore');
  const request = ui.evaluate('composerRequest()');
  assert.deepEqual([...request.to], ['"Ivanov, Petr" <p@example.test>'], 'the restored address is not sent as one address');
});

test('S-029: the editor keeps the text outside elements, plain styles and plain links of the letter', async () => {
  const ui = await openMail();
  const clean = html => ui.evaluate(`window.editorSafeHtml(${JSON.stringify(html)})`);
  assert.equal(clean('Позвоните в пятницу'), 'Позвоните в пятницу', 'the text outside elements was lost');
  assert.equal(clean('До<p>Середина</p>После'), 'До<p>Середина</p>После', 'the text next to elements was lost');
  // A form, a button and the like are shells around ordinary text: the shell goes, the text stays.
  assert.equal(clean('До<form><p>Номер заказа 42</p></form>После<button>Отправить</button>'), 'До<p>Номер заказа 42</p>ПослеОтправить', 'the text inside a form was lost with the form');
  assert.equal(clean('<p contenteditable="false" tabindex="5" draggable="true">x</p>'), '<p>x</p>', 'an attribute that changes the work of the editor stayed');
  // A style that names an outside picture, hides a property behind an escape or lays the body over the program goes;
  // the look of the text, of its frame and of the space around it stays.
  const styled = '<p style="color:red;list-style-image:url(https://tracker.example.test/p.png);position:fixed;top:0">x</p>'
    + '<div style="\\62 ackground:url(https://tracker.example.test/q.png)">y</div>'
    + '<q style="margin:4px;border-left:2px solid var(--border,#ccc);background-color:rgb(1, 2, 3)">z</q>'
    + '<p style="width:expression(alert(1))">w</p>';
  const kept = clean(styled);
  assert.equal(/tracker|position|expression/.test(kept), false, `a dangerous style stayed: ${kept}`);
  assert.match(kept, /^<p style="color: red">x<\/p><div>y<\/div><q style="margin: 4px; border-left: 2px solid var\(--border,#ccc\); background-color: rgb\(1, 2, 3\)">z<\/q><p>w<\/p>$/, kept);
  // The scheme of a link is read the way the browser reads it: blanks and control characters do not hide it.
  const links = clean('<a href="java\nscript:x()">a</a><a href="\u0001javascript:y()">b</a><a href="vbscript:z()">c</a>'
    + '<a href="https://example.test">d</a><a href="mailto:a@example.test">e</a><a href="#part">f</a>');
  assert.equal(/script/.test(links), false, `a link with an executable scheme stayed: ${links}`);
  assert.match(links, /<a href="https:\/\/example\.test">d<\/a>/, 'a plain link was lost');
  assert.match(links, /<a href="mailto:a@example.test">e<\/a>/, 'a mail link was lost');
  assert.match(links, /<a href="#part">f<\/a>/, 'a link inside the letter was lost');
});

test('S-006: a reply from the list menu takes the address, the subject and the quote of the letter under the cursor', async () => {
  const ui = await openMail();
  ui.evaluate("currentFolderId=2;applyListOptions(true,'Входящие')");
  await openLetter(ui, 11);
  openListMenu(ui, 12);
  ui.query('#ctxmenu [data-context-action="reply"]').dispatch('click');
  await settle();
  assert.equal(ui.byId('compSubj').value, 'Re: Тема B', 'the reply took the subject of the shown letter');
  assert.equal(ui.evaluate('recipientModel.compTo[0]?.email'), 'bob@example.test', 'the reply went to the sender of the shown letter');
  assert.match(ui.byId('compEdit').innerHTML, /Текст B/, 'the reply quotes the shown letter');
  assert.equal(ui.evaluate('activeMessage.id'), 11, 'the letter of the reading pane is not active again');

  // A new letter started while the other letter is being read stays as it is.
  let release = null;
  const other = await openMail({getMessage: id => (id === 12 ? new Promise(resolve => { release = resolve; }) : fullOf(id))});
  other.evaluate("currentFolderId=2;applyListOptions(true,'Входящие')");
  await openLetter(other, 11);
  openListMenu(other, 12);
  other.query('#ctxmenu [data-context-action="forward"]').dispatch('click');
  await settle();
  other.evaluate("document.getElementById('composeBtn').onclick()");
  await settle();
  other.evaluate("document.getElementById('compSubj').value='Моё письмо'");
  release(fullOf(12));
  await settle();
  assert.equal(other.byId('compSubj').value, 'Моё письмо', 'the late answer put another letter over the new one');
});

test('S-006: the sender commands of the list menu take the letter under the cursor', async () => {
  const ui = await openMail();
  ui.evaluate("currentFolderId=2;applyListOptions(true,'Входящие')");
  await openLetter(ui, 11);
  openListMenu(ui, 12);
  ui.query('#ctxmenu [data-context-action="block-sender"]').dispatch('click');
  await settle();
  const dialog = ui.queryAll('.overlay.open').map(node => node.textContent).join(' ');
  assert.match(dialog, /bob@example\.test/, `the sender of another letter is blocked: ${dialog.slice(0, 200)}`);
  assert.equal(/boss@example\.test/.test(dialog), false, 'the sender of the shown letter is offered for blocking');
});

test('S-006: an item of the list menu takes the letter under the cursor, not another selection', async () => {
  const ui = await openMail();
  await openLetter(ui, 21);
  ui.evaluate('selectedMessageIds.add(21)');
  openListMenu(ui, 22);
  ui.query('#ctxmenu [data-context-action="trash"]').dispatch('click');
  await settle();
  assert.deepEqual(trashed(ui), [[[22], 'trash']], 'another selection was deleted instead of the letter under the cursor');
  // The selection counts when the letter under the cursor is one of the selected.
  ui.evaluate('selectedMessageIds.add(21);selectedMessageIds.add(22)');
  openListMenu(ui, 22);
  ui.query('#ctxmenu [data-context-action="trash"]').dispatch('click');
  await settle();
  assert.deepEqual(trashed(ui).at(-1), [[21, 22], 'trash'], 'the selection of the selected letter was not deleted');
});

test('S-019: a letter taken by the queue is not sent again by either button', async () => {
  let release = null;
  const ui = await openMail({scheduleMessage: () => new Promise(resolve => { release = resolve; })});
  await openLetter(ui, 22);
  ui.byId('threadSendDraft').onclick();
  await settle();
  ui.evaluate("setRecipients('compTo',['mate@example.test']);const at=document.getElementById('compSendAt');at.classList.remove('hidden');at.value='2030-01-01T09:00';");
  ui.byId('compSendLater').onclick();
  await settle();
  // The ordinary sending while the queue is answering the scheduling.
  ui.byId('compSend').onclick();
  await settle();
  assert.equal(ui.callsOf('sendMessage').length, 0, 'the letter went to the queue twice');
  release(77);
  await settle();
  // Both buttons again after the answer, before the editor is cleared.
  ui.byId('compSendLater').onclick();
  ui.byId('compSend').onclick();
  await settle();
  assert.equal(ui.callsOf('scheduleMessage').length, 1, 'the letter was scheduled twice');
  assert.equal(ui.callsOf('sendMessage').length, 0, 'the scheduled letter was also sent at once');
  assert.equal(ui.byId('composeStatus').textContent, 'Запланировано (задача 77)', 'the user is not told that the letter is scheduled');
  assert.deepEqual(trashed(ui), [[[22], 'trash']], 'the scheduled letter left its draft in Drafts');
  await ui.clock.advance(800);
  assert.equal(ui.evaluate('composerHasContent()'), false, 'the editor was not cleared after the scheduling');
  // The next letter of the editor is sent as usual.
  ui.evaluate("setRecipients('compTo',['mate@example.test'])");
  await ui.byId('compSend').onclick();
  await settle();
  assert.equal(ui.callsOf('sendMessage').length, 1, 'the next letter could not be sent');
});

test('S-014: a failure to clear the saved copy does not undo the scheduling of a draft', async () => {
  const ui = await openMail({
    scheduleMessage: () => 77,
    setSetting: (key, value) => (key === 'composer_draft' && value === '' ? Promise.reject(new Error('хранилище недоступно')) : null),
  });
  await openLetter(ui, 22);
  ui.byId('threadSendDraft').onclick();
  await settle();
  ui.evaluate("setRecipients('compTo',['mate@example.test']);const at=document.getElementById('compSendAt');at.classList.remove('hidden');at.value='2030-01-01T09:00';");
  ui.byId('compSendLater').onclick();
  await settle();
  assert.equal(ui.callsOf('scheduleMessage').length, 1);
  assert.deepEqual(trashed(ui), [[[22], 'trash']], 'the draft stayed in the folder because the saved copy was not cleared');
  assert.equal(ui.byId('composeStatus').dataset.kind, 'success', 'the scheduling is reported as a failure');
  assert.ok(ui.callsOf('uiLog').some(call => /was not cleared/.test(call.args[0])), 'the failure to clear the saved copy is not written to the log');
  ui.byId('compSendLater').onclick();
  await settle();
  assert.equal(ui.callsOf('scheduleMessage').length, 1, 'the scheduled letter was scheduled again');
});

test('S-010: a letter of markup alone keeps an empty text part on the way through the editor', async () => {
  const ui = await openMail({getMessage: id => ({...fullOf(id), has_text_part: false})});
  await openLetter(ui, 22);
  ui.byId('threadSendDraft').onclick();
  await settle();
  ui.evaluate("setRecipients('compTo',['mate@example.test'])");
  await ui.byId('compSend').onclick();
  await settle();
  assert.equal(sent(ui)[0].body_text, '', 'the text part was made from the markup in the editor');

  // Typing into the body makes the text of the letter its own.
  const typed = await openMail({getMessage: id => ({...fullOf(id), has_text_part: false})});
  await openLetter(typed, 22);
  typed.byId('threadSendDraft').onclick();
  await settle();
  typed.byId('compEdit').dispatch('input');
  typed.evaluate("setRecipients('compTo',['mate@example.test'])");
  await typed.byId('compSend').onclick();
  await settle();
  assert.equal(sent(typed)[0].body_text, 'Текст письма', 'the typed letter lost its text part');
});

test('S-012: a letter restored from the saved state still takes its draft out of the folder', async () => {
  const ui = await openMail();
  ui.evaluate("resetComposer();window.pendingComposerDraft={account_id:2,to:[{name:'Mate',email:'mate@example.test'}],cc:[],bcc:[],subject:'Не отправлено',body_html:'<p>Текст письма</p>',attachments:[],draft_message_id:22};renderAccountSettings(coreAccounts,[],[])");
  await settle();
  await ui.byId('compSend').onclick();
  await settle();
  assert.equal(sent(ui).length, 1);
  assert.deepEqual(trashed(ui), [[[22], 'trash']], 'the restored letter left its draft in Drafts');

  // A draft that no longer exists gives no link: the letter is an ordinary one.
  const gone = await openMail({getMessage: () => Promise.reject(new Error('письмо не найдено'))});
  gone.evaluate("resetComposer();window.pendingComposerDraft={account_id:2,to:[{name:'Mate',email:'mate@example.test'}],cc:[],bcc:[],subject:'Не отправлено',body_html:'<p>Текст письма</p>',attachments:[],draft_message_id:22};renderAccountSettings(coreAccounts,[],[])");
  await settle();
  await gone.byId('compSend').onclick();
  await settle();
  assert.equal(sent(gone).length, 1);
  assert.equal(trashed(gone).length, 0, 'a letter was moved to Trash for a draft that does not exist');
});

test('S-016: another draft is sent while the first one is being read', async () => {
  const readers = [];
  let readings = 0;
  const addressed = id => ({...fullOf(id), meta: {...LETTERS[id], to: id === 22 ? [{name: 'Mate', email: 'mate@example.test'}] : LETTERS[id].to}});
  // The first reading shows the first draft; the sending of it is released by the test.
  const ui = await openMail({getMessage: id => {
    if (id !== 21) return addressed(id);
    readings += 1;
    if (readings === 1) return addressed(id);
    return new Promise(resolve => { readers.push(() => resolve(addressed(id))); });
  }});
  await openLetter(ui, 21);
  ui.byId('threadSendDraft').onclick();
  await openLetter(ui, 22);
  ui.byId('threadSendDraft').onclick();
  await settle();
  assert.deepEqual(trashed(ui).map(call => call[0]), [[22]], 'the second draft did not go out while the first one is being read');
  readers[0]();
  await settle();
  assert.equal(sent(ui).length, 2, 'the first draft was not sent');
  assert.deepEqual(trashed(ui).map(call => call[0]).sort(), [[21], [22]], 'a draft stayed in its folder');
});

test('S-017: a late show of the accepted draft does not bring it back', async () => {
  const shows = [];
  let releaseQueue = null;
  let reads = 0;
  const ui = await openMail({
    getMessage: id => {
      reads += 1;
      if (reads === 2) return new Promise(resolve => { shows.push(() => resolve(fullOf(id))); });
      return fullOf(id);
    },
    sendMessage: () => new Promise(resolve => { releaseQueue = resolve; }),
  });
  await openLetter(ui, 21);
  // The same draft is shown again, and the reading for the show is late.
  ui.evaluate('showMessage(messages.find(message=>message.id===21))');
  await settle();
  ui.byId('threadSendDraft').onclick();
  await settle();
  releaseQueue({account_id: 1, operation_id: 501, cancel_until: null});
  await settle();
  shows[0]();
  await settle();
  assert.equal(ui.byId('tbody').innerHTML, '', 'the accepted draft came back over the empty subject');
  assert.equal(ui.evaluate('paneLetter()'), null, 'the accepted draft is the shown letter again');
});

test('S-002: an empty reading pane has no active letter for the buttons to act on', async () => {
  const ui = await openMail();
  await openLetter(ui, 21);
  // Another folder: the shown draft is not in its list, and the pane is cleared.
  ui.evaluate("currentFolderId=2;applyListOptions(true,'Входящие')");
  ui.query('.thead [data-act="archive"]').dispatch('click');
  await settle();
  assert.equal(ui.callsOf('messageAction').length, 0, 'a button of the empty pane acted on a letter that is not on the screen');
  // With a selection in the list the same button acts on the selection, as it always did.
  ui.evaluate('selectedMessageIds.add(11);selectedMessageIds.add(12)');
  ui.query('.thead [data-act="archive"]').dispatch('click');
  await settle();
  assert.deepEqual(trashed(ui), [[[11, 12], 'archive']], 'the button lost the selection of the list');
});

test('S-019: a late answer for another letter does not open the scheduled letter again', async () => {
  let releaseSend = null;
  const ui = await openMail({
    sendMessage: () => new Promise(resolve => { releaseSend = resolve; }),
    scheduleMessage: () => 77,
  });
  await openLetter(ui, 22);
  ui.byId('threadSendDraft').onclick();
  await settle();
  ui.evaluate("setRecipients('compTo',['mate@example.test'])");
  ui.byId('compSend').onclick();
  await settle();
  // Another letter of the editor is scheduled while the queue answers the first one.
  ui.evaluate("document.getElementById('composeBtn').onclick()");
  await settle();
  ui.evaluate("setRecipients('compTo',['other@example.test']);document.getElementById('compSubj').value='Другое';const at=document.getElementById('compSendAt');at.classList.remove('hidden');at.value='2030-01-01T09:00';");
  ui.byId('compSendLater').onclick();
  await settle();
  releaseSend({account_id: 1, operation_id: 501, cancel_until: null});
  await settle();
  ui.byId('compSend').onclick();
  ui.byId('compSendLater').onclick();
  await settle();
  assert.equal(ui.callsOf('sendMessage').length, 1, 'the scheduled letter went to the queue a second time');
  assert.equal(ui.callsOf('scheduleMessage').length, 1, 'the scheduled letter was scheduled a second time');
  assert.equal(ui.byId('compSubj').value, 'Другое', 'the answer for the old letter wiped the new one');
  const saved = ui.callsOf('setSetting').filter(call => call.args[0] === 'composer_draft').at(-1);
  assert.equal(saved?.args[1], '', 'the answer for the old letter saved the scheduled letter again');
});

test('S-019: the answer for an old letter does not cancel the saving of a new one', async () => {
  let releaseSend = null;
  const ui = await openMail({sendMessage: () => new Promise(resolve => { releaseSend = resolve; })});
  await openLetter(ui, 22);
  ui.byId('threadSendDraft').onclick();
  await settle();
  ui.evaluate("setRecipients('compTo',['mate@example.test'])");
  ui.byId('compSend').onclick();
  await settle();
  // A new letter is being written, and its saving is due in half a second.
  ui.evaluate("document.getElementById('composeBtn').onclick()");
  await settle();
  ui.evaluate("setRecipients('compTo',['other@example.test']);document.getElementById('compSubj').value='Письмо B';scheduleDraftSave();");
  releaseSend({account_id: 1, operation_id: 501, cancel_until: null});
  await settle();
  const savedCopy = () => {
    const call = ui.callsOf('setSetting').filter(item => item.args[0] === 'composer_draft').at(-1);
    return call ? call.args[1] : null;
  };
  const rightAfter = JSON.parse(savedCopy());
  assert.equal(rightAfter.subject, 'Письмо B', 'the copy of the sent letter stayed in place of the new one');
  await ui.clock.advance(600);
  const later = JSON.parse(savedCopy());
  assert.equal(later.subject, 'Письмо B', 'the saving of the new letter was cancelled by the answer for the old one');
});

test('S-033: a letter opened while an action of the menu runs stays the one on the screen', async () => {
  let release = null;
  const ui = await openMail({messageAction: () => new Promise(resolve => { release = resolve; })});
  ui.evaluate("currentFolderId=2;applyListOptions(true,'Входящие')");
  await openLetter(ui, 11);
  openListMenu(ui, 12);
  ui.query('#ctxmenu [data-context-action="trash"]').dispatch('click');
  await settle();
  await openLetter(ui, 13);
  release({operation_ids: [1012], skipped: 0});
  await settle();
  assert.equal(ui.evaluate('paneLetter()?.id'), 13, 'the letter on the screen was cleared by the action on another letter');
  assert.equal(ui.evaluate('activeMessage.id'), 13, 'the letter shown before the action became the active one again');
});

test('S-012: the link of a restored letter goes only to a letter that is still a draft', async () => {
  const ui = await openMail();
  ui.evaluate("resetComposer();window.pendingComposerDraft={account_id:1,to:[{name:'Mate',email:'mate@example.test'}],cc:[],bcc:[],subject:'Письмо',body_html:'<p>Текст письма</p>',attachments:[],draft_message_id:11};renderAccountSettings(coreAccounts,[],[])");
  await settle();
  await ui.byId('compSend').onclick();
  await settle();
  assert.equal(sent(ui).length, 1);
  assert.equal(trashed(ui).length, 0, 'an incoming letter was moved to Trash as if it were the draft');

  // Nothing is sent before the check of the draft has answered.
  let release = null;
  const slow = await openMail({getMessage: id => (id === 22 ? new Promise(resolve => { release = resolve; }) : fullOf(id))});
  slow.evaluate("resetComposer();window.pendingComposerDraft={account_id:2,to:[{name:'Mate',email:'mate@example.test'}],cc:[],bcc:[],subject:'Не отправлено',body_html:'<p>Текст письма</p>',attachments:[],draft_message_id:22};renderAccountSettings(coreAccounts,[],[])");
  slow.byId('compSend').onclick();
  await settle();
  assert.equal(sent(slow).length, 0, 'the letter went out before the check of its draft answered');
  release(fullOf(22));
  await settle();
  assert.deepEqual(trashed(slow), [[[22], 'trash']], 'the draft stayed in its folder');
});

test('S-010: a restored letter keeps the empty text part until the body is typed into', async () => {
  const ui = await openMail();
  ui.evaluate("resetComposer();window.pendingComposerDraft={account_id:2,to:[{name:'Mate',email:'mate@example.test'}],cc:[],bcc:[],subject:'Письмо',body_html:'<p>Только HTML</p>',attachments:[],text_missing:true};renderAccountSettings(coreAccounts,[],[])");
  assert.equal(ui.evaluate('composerRequest().body_text'), '', 'the restored letter got a text part made from the markup');
  ui.byId('compEdit').dispatch('input');
  assert.equal(ui.evaluate('composerRequest().body_text'), 'Только HTML', 'the typed letter lost its text part');
});

test('S-025: the fields of the copy stay hidden after the restore when they hold no address', async () => {
  const ui = await openMail();
  ui.evaluate("resetComposer();window.pendingComposerDraft={account_id:1,to:[{name:'',email:'a@example.test'}],cc:[],bcc:[{name:'',email:'b@example.test'}],subject:'',body_html:'',attachments:[]};renderAccountSettings(coreAccounts,[],[])");
  assert.equal(ui.query('[data-recipient-field="compCc"]').classes.has('hidden'), true, 'the empty field of the copy was shown');
  assert.equal(ui.query('[data-recipient-field="compBcc"]').classes.has('hidden'), false, 'the field of the hidden copy with an address stayed hidden');
});

test('S-024: a name with a bracket survives the way through the editor', async () => {
  const ui = await openMail({
    getMessage: id => ({...fullOf(id), meta: {...LETTERS[id], to: [{name: 'Dev (East', email: 'mate@example.test'}]}, attachments: [part(0)]}),
    attachmentContent: () => Promise.reject(new Error('вложение не читается')),
  });
  await openLetter(ui, 21);
  ui.byId('threadSendDraft').onclick();
  await settle();
  assert.equal(ui.evaluate('recipientModel.compTo[0]?.name'), 'Dev (East', 'the name with a bracket was cut into a wrong address');
  assert.equal(ui.evaluate('recipientModel.compTo[0]?.email'), 'mate@example.test', 'the address next to a name with a bracket was damaged');
  await ui.byId('compSend').onclick();
  await settle();
  assert.deepEqual([...sent(ui)[0].to], ['"Dev (East" <mate@example.test>'], 'the name with a bracket is not sent as one address');
});

test('S-036: snoozing from the list menu takes the letter under the cursor, not another selection', async () => {
  const ui = await openMail({snoozeMessages: () => ({})});
  ui.evaluate("currentFolderId=2;applyListOptions(true,'Входящие')");
  await openLetter(ui, 11);
  ui.evaluate('selectedMessageIds.add(11)');
  openListMenu(ui, 12);
  ui.query('#ctxmenu [data-context-action="snooze"]').dispatch('click');
  await settle();
  ui.query('.snooze-modal [data-offset="hour"]').dispatch('click');
  await settle();
  assert.deepEqual(ui.callsOf('snoozeMessages').map(call => [[...call.args[0]]]), [[[12]]], 'the selection was snoozed instead of the letter under the cursor');
  assert.equal(ui.evaluate('paneLetter()?.id'), 11, 'the letter that was not snoozed left the screen');
});

test('S-036: the list menu of a row acts on the letter of that row and on nothing else', async () => {
  const ui = await openMail();
  await openLetter(ui, 21);
  // The letter of the row is forgotten by the list, the row itself is still drawn.
  ui.evaluate('window.forgetMessages?.([22])');
  openListMenu(ui, 22);
  ui.query('#ctxmenu [data-context-action="trash"]').dispatch('click');
  await settle();
  assert.deepEqual(trashed(ui), [[[22], 'trash']], 'the menu of the forgotten letter acted on the shown one');
  // The letter is gone from every list of the program: the menu does not open for its row.
  ui.evaluate('messages=messages.filter(message=>message.id!==21);currentMessageRows=currentMessageRows.filter(message=>message?.id!==21);lastListRows=lastListRows.filter(message=>message?.id!==21);');
  const before = ui.callsOf('messageAction').length;
  ui.query('#msgs .msg[data-message-id="21"]').dispatch('contextmenu', {clientX: 20, clientY: 20});
  assert.equal(ui.byId('ctxmenu').classes.has('open'), false, 'the menu opened for a letter that is gone');
  assert.equal(ui.callsOf('messageAction').length, before, 'a command went to another letter from the menu of a letter that is gone');
});

test('S-014: a failure to clear the saved copy after the ordinary sending is written to the log', async () => {
  const ui = await openMail({setSetting: (key, value) => (key === 'composer_draft' && value === '' ? Promise.reject(new Error('хранилище недоступно')) : null)});
  await openLetter(ui, 22);
  ui.byId('threadSendDraft').onclick();
  await settle();
  ui.evaluate("setRecipients('compTo',['mate@example.test'])");
  await ui.byId('compSend').onclick();
  await settle();
  assert.equal(sent(ui).length, 1);
  assert.deepEqual(trashed(ui), [[[22], 'trash']], 'the draft stayed in its folder');
  assert.ok(ui.callsOf('uiLog').some(call => /was not cleared/.test(call.args[0])), 'the failure to clear the saved copy is not written to the log');
});

test('S-036: unsubscribing by email from the list menu preserves the unsent letter', async () => {
  const ui = await openMail({
    getMessage: id => id === 12
      ? {...fullOf(id), body_html: null, unsubscribe: {one_click_url: null, http: null, mailto: 'mailto:stop@example.test'}}
      : fullOf(id),
  });
  ui.evaluate("currentFolderId=2;applyListOptions(true,'Входящие')");
  await openLetter(ui, 11);
  ui.evaluate("resetComposer();setRecipients('compTo',['other@example.test']);document.getElementById('compSubj').value='Моё письмо';showView('mailView');");
  openListMenu(ui, 12);
  ui.query('#ctxmenu [data-context-action="unsub"]').dispatch('click');
  await settle();
  assert.equal(ui.byId('compSubj').value, 'Моё письмо');
  assert.ok(notices(ui).some(text => text.includes('неотправленное письмо')));
});

test('S-012: a file chosen while the draft link is checked goes with the letter', async () => {
  let releaseLink, releaseFile;
  const ui = await openMail({
    getMessage: () => new Promise(resolve => { releaseLink = resolve; }),
    readLocalFile: () => new Promise(resolve => { releaseFile = resolve; }),
  });
  ui.evaluate("resetComposer();window.pendingComposerDraft={account_id:2,to:[{name:'Mate',email:'mate@example.test'}],cc:[],bcc:[],subject:'Письмо',body_html:'<p>Текст письма</p>',attachments:[],draft_message_id:22};renderAccountSettings(coreAccounts,[],[])");
  const sending = ui.byId('compSend').onclick();
  await settle();
  const attaching = ui.evaluate("window.composeWithFiles(['C:/tmp/plan.pdf'])");
  await settle();
  releaseLink(fullOf(22));
  await settle();
  assert.equal(sent(ui).length, 0, 'the letter went out before the file was read');
  releaseFile({filename: 'plan.pdf', mime_type: 'application/pdf', base64: 'QQ=='});
  await attaching;
  await sending;
  assert.equal(sent(ui).length, 1);
  assert.equal(sent(ui)[0].attachments.length, 1);
  assert.equal(sent(ui)[0].attachments[0].filename, 'plan.pdf');
});

test('S-010: applying a template supplies the text part of a restored letter', async () => {
  const ui = await openMail({
    listMessageTemplates: () => [{id: 1, name: 'Шаблон', subject: 'Тема', body_html: '<p>Текст шаблона</p>'}],
  });
  ui.evaluate("resetComposer();window.pendingComposerDraft={account_id:2,to:[{name:'Mate',email:'mate@example.test'}],cc:[],bcc:[],subject:'Письмо',body_html:'<p>Только HTML</p>',attachments:[],text_missing:true};renderAccountSettings(coreAccounts,[],[])");
  assert.equal(ui.evaluate('composerRequest().body_text'), '');
  await ui.byId('compTemplates').onclick();
  await ui.query('.template-row .btn').onclick();
  assert.equal(ui.evaluate('composerRequest().body_text'), 'Текст шаблона');
});
