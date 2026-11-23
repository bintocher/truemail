// A draft in the reading pane, in its "more" menu and in the list menu, and its
// way out through the composer: the real markup, the real handlers and the
// bridge. Before, a draft offered the actions of an incoming letter and could
// not be sent from the program at all.
// Specification: specs/draft-message-actions.md.
// Run: node --test apps/desktop/tests/js/draft-message.test.js (Node 22+).

'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const {createUiWindow} = require('./ui-window.js');
const draftMessageModel = require('../../ui/modules/draft-message.js');

const ACCOUNT = {id: 1, email: 'me@example.test', display_name: 'Me'};
const FOLDERS = [
  {id: 2, account_id: 1, role: 'inbox', display_name: 'Входящие', remote_path: 'INBOX'},
  {id: 3, account_id: 1, role: 'drafts', display_name: 'Черновики', remote_path: 'Drafts'},
];
const INCOMING = {id: 11, account_id: 1, folder_id: 2, subject: 'Входящее', preview: '',
  from: {name: 'Boss', email: 'boss@example.test'}, to: [{name: '', email: 'me@example.test'}], cc: [],
  flags: {seen: true, flagged: false, draft: false}, labels: [], date: '2026-11-23T10:00:00Z', pinned_at: null};
const DRAFT = {id: 21, account_id: 1, folder_id: 3, subject: 'Не отправлено', preview: '',
  from: {name: 'Me', email: 'me@example.test'}, to: [{name: 'Mate', email: 'mate@example.test'}], cc: [],
  flags: {seen: true, flagged: false, draft: true}, labels: [], date: '2026-11-23T09:00:00Z', pinned_at: null};
// No draft flag and no recipient: the folder alone tells that it is a draft.
const UNADDRESSED = {...DRAFT, id: 22, to: [], flags: {seen: true, flagged: false, draft: false}};
const LETTERS = {11: INCOMING, 21: DRAFT, 22: UNADDRESSED};

const fullOf = id => ({
  meta: LETTERS[id],
  body_html: '<p>Текст письма</p>', body_text: 'Текст письма', attachments: [],
  has_remote_content: false, is_newsletter: false, unsubscribe: null, body_fetched: true,
  bcc: [{name: 'Hidden', email: 'hidden@example.test'}],
});

const settle = async () => {
  for (let round = 0; round < 12; round += 1) await new Promise(resolve => setImmediate(resolve));
};
const shown = node => Boolean(node) && !node.classList.contains('hidden') && !node.classList.contains('toolbar-original-hidden');
const paneActions = ui => ui.queryAll('.thead [data-act]').filter(shown).map(node => node.dataset.act);
const menuActions = ui => ui.queryAll('#ctxmenu [data-context-action]').map(node => node.dataset.contextAction);

async function openMail(answers = {}) {
  const ui = createUiWindow({
    answers: {
      getMessage: id => fullOf(id),
      sendMessage: () => ({account_id: 1, operation_id: 501, cancel_until: null}),
      messageAction: ids => ({operation_ids: ids.map(id => id + 1000), skipped: 0}),
      ...answers,
    },
    globals: {reloadCoreData: async () => {}},
  });
  await ui.clock.drain();
  await ui.evaluate('window.localizationReady');
  ui.evaluate(`
    coreAccounts=[${JSON.stringify(ACCOUNT)}];
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
// Messages for the user are entries of the event journal in the status line.
const notices = ui => ui.queryAll('#activityPanel .activity-entry-text').map(node => node.textContent);
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

test('S-002, S-003, S-004: the pane and both menus follow the letter, a ready draft goes out by the composer path', async () => {
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

  ui.byId('threadSendDraft').onclick();
  await settle();
  const request = sent(ui)[0];
  assert.equal(request.account_id, 1);
  assert.deepEqual([...request.to], ['Mate <mate@example.test>']);
  assert.deepEqual([...request.bcc], ['Hidden <hidden@example.test>'], 'the hidden copy of the draft was lost');
  assert.equal(request.subject, 'Не отправлено');
  assert.equal(request.body_html, '<p>Текст письма</p>');
  assert.deepEqual(trashed(ui), [[[21], 'trash']], 'the sent draft stayed in Drafts');
  assert.equal(paneActions(ui).includes('send-draft'), false, 'Send stays on the pane after the draft went out');
  assert.equal(ui.byId('composeView').classes.has('active'), false, 'the editor opened for a ready draft');

  // An ordinary letter keeps every action it had.
  await openLetter(ui, 11);
  assert.equal(paneActions(ui).includes('send-draft'), false, 'an incoming letter offers Send');
  for (const action of ['reply', 'replyall', 'forward', 'archive']) {
    assert.equal(paneActions(ui).includes(action), true, `an incoming letter lost "${action}"`);
  }
  assert.equal(shown(ui.query('#threadMoreMenu [data-thread-action="create-rule"]')), true);
  assert.equal(ui.queryAll('#threadMoreMenu .hidden').length, 0, 'an incoming letter lost an item of the "more" menu');

  // The sent draft is forgotten by the list, so the menu is checked on the draft still in it.
  const draftMenu = openListMenu(ui, 22);
  assert.equal(draftMenu[0], 'send-draft', 'the list menu of a draft does not start with Send');
  for (const action of ['reply', 'archive', 'create-rule', 'block-sender', 'ignore-conversation', 'sweep-sender']) {
    assert.equal(draftMenu.includes(action), false, `the list menu of a draft offers "${action}"`);
  }
  assert.equal(draftMenu.includes('trash'), true, 'the list menu of a draft cannot delete it');
  assert.equal(draftMenu.includes('raw'), true, 'the list menu of a draft lost its source view');

  ui.evaluate("currentFolderId=2;applyListOptions(true,'Входящие')");
  const incomingMenu = openListMenu(ui, 11);
  assert.equal(incomingMenu.includes('send-draft'), false, 'an incoming letter offers Send in its menu');
  assert.equal(incomingMenu.includes('reply'), true, 'an incoming letter lost Reply in its menu');
  assert.equal(incomingMenu.includes('create-rule'), true, 'an incoming letter lost its rule action');

  // Once the open draft is gone, the pane is the pane of any other letter.
  await openLetter(ui, 22);
  assert.equal(paneActions(ui).includes('send-draft'), true);
  ui.evaluate("performMessageAction('trash')");
  await settle();
  assert.equal(paneActions(ui).includes('send-draft'), false, 'Send stays on the pane after the draft was deleted');
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

test('S-007: another unsent letter in the editor is not overwritten by a draft', async () => {
  const ui = await openMail();
  await openLetter(ui, 21);
  ui.evaluate("resetComposer();setRecipients('compTo',['other@example.test']);document.getElementById('compSubj').value='Другое письмо';");
  ui.byId('threadSendDraft').onclick();
  await settle();
  assert.equal(sent(ui).length, 0, 'the draft was sent over another letter');
  assert.equal(ui.byId('compSubj').value, 'Другое письмо', 'the draft replaced the letter being written');
  assert.equal(ui.byId('composeView').classes.has('active'), true, 'the user was not brought to the letter being written');
  assert.equal(trashed(ui).length, 0);
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

test('S-012: a letter started while the attachments of the draft are read stays, and the draft is not sent', async () => {
  const readers = [];
  const ui = await openMail({
    getMessage: id => ({...fullOf(id), attachments: [{id: 0, filename: 'a.pdf', mime_type: 'application/pdf'}]}),
    attachmentContent: () => new Promise(resolve => { readers.push(resolve); }),
  });
  await openLetter(ui, 21);
  ui.byId('threadSendDraft').onclick();
  await settle();
  assert.equal(readers.length, 1, 'the attachment of the draft is not being read');
  ui.evaluate("document.getElementById('composeBtn').onclick()");
  await settle();
  ui.evaluate("document.getElementById('compSubj').value='Моё письмо'");
  readers[0]({filename: 'a.pdf', mime_type: 'application/pdf', base64: 'QQ=='});
  await settle();
  assert.equal(ui.byId('compSubj').value, 'Моё письмо', 'the draft replaced the letter being written');
  assert.equal(ui.evaluate('composerAttachments.length'), 0, 'an attachment of the draft landed in another letter');
  assert.equal(ui.byId('composeView').classes.has('active'), true);
  assert.equal(sent(ui).length, 0, 'the draft was sent over another letter');
  assert.equal(trashed(ui).length, 0);
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

test('S-007: a draft that could not be moved to Trash is reported', async () => {
  const ui = await openMail({messageAction: () => ({operation_ids: [], skipped: 1, skipped_no_folder: 1})});
  await openLetter(ui, 21);
  ui.byId('threadSendDraft').onclick();
  await settle();
  assert.equal(sent(ui).length, 1, 'the letter must go out even when the draft cannot be removed');
  assert.ok(notices(ui).some(text => /вручную/.test(text)), 'the draft left in its folder is not reported');
  assert.equal(ui.callsOf('uiLog').length, 1, 'the failure is not written to the program log');
});

test('S-009: after the list menu the pane follows the letter under the cursor', async () => {
  const ui = await openMail();
  await openLetter(ui, 21);
  ui.evaluate("currentFolderId=2;applyListOptions(true,'Входящие')");
  openListMenu(ui, 11);
  assert.equal(paneActions(ui).includes('send-draft'), false, 'Send stays above an ordinary letter');
  assert.equal(paneActions(ui).includes('reply'), true, 'an ordinary letter lost Reply');
  ui.evaluate("currentFolderId=3;applyListOptions(true,'Черновики')");
  await openLetter(ui, 11);
  openListMenu(ui, 21);
  assert.equal(paneActions(ui).includes('send-draft'), true, 'the draft under the cursor has no Send button');
  assert.equal(paneActions(ui).includes('reply'), false, 'the draft under the cursor is offered a reply');
});

test('S-005: attachments go along once, a name with a comma is one address', async () => {
  const ui = await openMail({
    getMessage: id => ({
      ...fullOf(id),
      meta: {...LETTERS[id], to: [{name: 'Ivanov, Petr', email: 'p@example.test'}]},
      attachments: [
        {id: 0, filename: 'a.pdf', mime_type: 'application/pdf', is_inline: false, content_id: null},
        {id: 1, filename: 'pic.png', mime_type: 'image/png', is_inline: true, content_id: 'pic@example.test'},
      ],
    }),
    attachmentContent: () => ({filename: 'a.pdf', mime_type: 'application/pdf', base64: 'QQ=='}),
  });
  await openLetter(ui, 21);
  ui.byId('threadSendDraft').onclick();
  await settle();
  const request = sent(ui)[0];
  assert.deepEqual([...request.to], ['Ivanov, Petr <p@example.test>'], 'a name with a comma was cut into two addresses');
  assert.equal(request.attachments.length, 1, 'an inline picture of the body was attached a second time');
  assert.equal(ui.callsOf('attachmentContent').length, 1, 'an inline picture was read as a separate attachment');
});

test('S-006: when an attachment cannot be read, the open editor still removes the draft once the letter goes out', async () => {
  const ui = await openMail({
    getMessage: id => ({...fullOf(id), attachments: [{id: 0, filename: 'a.pdf', mime_type: 'application/pdf'}]}),
    attachmentContent: () => Promise.reject(new Error('вложение без содержимого')),
  });
  await openLetter(ui, 21);
  ui.byId('threadSendDraft').onclick();
  await settle();
  assert.equal(ui.byId('composeView').classes.has('active'), true, 'the editor did not open after the attachment failed');
  assert.equal(sent(ui).length, 0);
  assert.equal(trashed(ui).length, 0, 'the draft was removed before its letter went out');
  await ui.byId('compSend').onclick();
  await settle();
  assert.equal(sent(ui).length, 1);
  assert.deepEqual(trashed(ui), [[[21], 'trash']], 'the draft sent from the editor stayed in Drafts');
});

test('S-012: a letter started while the draft itself is being read stays, and the draft is not sent', async () => {
  const readers = [];
  let reads = 0;
  const ui = await openMail({getMessage: id => {
    reads += 1;
    if (reads === 1) return fullOf(id);
    return new Promise(resolve => { readers.push(() => resolve(fullOf(id))); });
  }});
  await openLetter(ui, 21);
  ui.byId('threadSendDraft').onclick();
  await settle();
  assert.equal(readers.length, 1, 'the draft is not being read');
  ui.evaluate("document.getElementById('composeBtn').onclick()");
  await settle();
  ui.evaluate("setRecipients('compTo',['other@example.test']);document.getElementById('compSubj').value='Моё письмо'");
  readers[0]();
  await settle();
  assert.equal(ui.byId('compSubj').value, 'Моё письмо', 'the draft replaced the letter being written');
  assert.equal(ui.evaluate("recipientModel.compTo.length"), 1, 'the draft removed the recipient of another letter');
  assert.equal(sent(ui).length, 0, 'the draft was sent over another letter');
  assert.equal(trashed(ui).length, 0);
  assert.ok(notices(ui).some(text => /другое письмо/.test(text)), 'the user is not told that the draft stayed unsent');
});
