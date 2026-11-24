'use strict';
const {test} = require('node:test');
const assert = require('node:assert/strict');
const {startApp} = require('./ui-window.js');

const accounts = [
  {id: 1, email: 'first@yandex.example'},
  {id: 2, email: 'second@yandex.example'},
];
const trash = '&BCMENAQwBDsENQQ9BD0ESwQ1-';
const drafts = '&BCcENQRABD0EPgQyBDgEOgQ4-';
const yandexFolders = [
  [
    [1, '&BB4EQgQ,BEAEMAQyBDsENQQ9BD0ESwQ1-', 'Отправленные', 'sent'],
    [2, '&BBAEQARFBDgEMg-', 'Архив', 'archive'],
    [3, '&BBgEQQRFBD4ENARPBEkEOAQ1-', 'Исходящие'],
    [4, '&BCEEPwQwBDw-', 'Спам', 'spam'],
    [5, trash, 'Удаленные', 'trash'],
    [6, `${trash}|pending`, 'pending', null, 5],
    [7, drafts, 'Черновики', 'drafts'],
    [8, `${drafts}|template`, 'template', null, 7],
    [9, 'Archive', 'Archive', 'archive'],
    [10, 'INBOX', 'INBOX', 'inbox'],
  ],
  [
    [11, drafts, 'Черновики', 'drafts'],
    [12, 'Archive', 'Archive', 'archive'],
    [13, 'Drafts', 'Drafts', 'drafts'],
    [14, 'INBOX', 'INBOX', 'inbox'],
    [15, 'Notes', 'Notes'],
    [16, 'Outbox', 'Outbox'],
    [17, 'Sent', 'Sent', 'sent'],
    [18, 'Spam', 'Spam', 'spam'],
    [19, 'Trash', 'Trash', 'trash'],
    [20, 'Trash|template', 'template', null, 19],
    [21, 'sd', 'sd'],
  ],
].map((folders, index) => folders.map(([id, remote_path, display_name, role = null, parent_id = null]) => ({
  id, account_id: accounts[index].id, remote_path, display_name, role, parent_id, unread_count: 0, total_count: 0,
})));

async function renderMail(list = accounts, folders = yandexFolders) {
  const app = startApp();
  await app.ready();
  app.sandbox.testAccounts = list;
  app.sandbox.testFoldersByAccount = folders;
  app.run('renderCoreAccounts(testAccounts, testFoldersByAccount)');
  return app;
}

const navRows = app => app.queryAll('.folder-row[data-folder-id]');
const folderName = row => row.querySelector('.folder-name').textContent;
const rowById = (app, id) => app.query(`.folder-row[data-folder-id="${id}"]`);

test('#137: подпапки двух ящиков Яндекса идут под родителями, одинаковые роли различимы', async () => {
  const app = await renderMail();
  assert.deepEqual(navRows(app).map(row => Number(row.dataset.folderId)), [
    10, 1, 7, 8, 2, 9, 4, 5, 6, 3,
    14, 17, 11, 13, 12, 18, 19, 20, 15, 16, 21,
  ]);
  for (const [parent, child] of [[5, 6], [7, 8], [19, 20]]) {
    assert.ok(parseInt(rowById(app, child).style.paddingLeft, 10) > parseInt(rowById(app, parent).style.paddingLeft, 10));
  }
  for (const [id, expected] of [
    [2, 'Архив'], [9, 'Archive'], [11, 'Черновики'], [13, 'Drafts'],
    [12, 'Архив'], [19, 'Удалённые'], [20, 'template'],
  ]) {
    assert.equal(folderName(rowById(app, id)), expected);
  }
  rowById(app, 9).onclick();
  assert.equal(app.query('.listhead h2').textContent, 'Archive');
});

test('#137: сопоставление, источники и выбор папки в правилах показывают полный путь', async () => {
  const app = await renderMail();
  for (const [id, path] of [[6, 'Удаленные / pending'], [8, 'Черновики / template'], [20, 'Trash / template']]) {
    assert.equal(app.query(`#set-folders option[value="${id}"]`).textContent, path);
    assert.equal(app.query(`[data-source-folder="${id}"]`).parentElement.querySelector('.source-path').textContent, path);
  }
  assert.equal(app.query('#set-folders option[value="19"]').textContent, 'Trash');

  app.run("openRuleEditor({account_id: 1})");
  const ruleOption = id => app.query(`.rule-action-folder option[value="folder:${id}"]`).textContent;
  assert.equal(ruleOption(6), 'Удаленные / pending');
  assert.equal(ruleOption(10), 'Входящие');
  assert.equal(ruleOption(9), 'Archive');
  app.run("openRuleEditor({account_id: 2})");
  assert.equal(ruleOption(20), 'Trash / template');
  app.query('#ruleRunOpen').onclick();
  assert.equal(app.query('#ruleRunFolders input[value="20"]').parentElement.querySelector('span').textContent, 'Trash / template');
});

test('#137: без родителя подпапка остаётся на верхнем уровне', async () => {
  const folders = yandexFolders[1].filter(folder => folder.id !== 19).map(folder => ({...folder, parent_id: null}));
  const app = await renderMail([accounts[1]], [folders]);
  const orphan = rowById(app, 20);
  assert.equal(folderName(orphan), 'template');
  assert.equal(orphan.style.paddingLeft, rowById(app, 15).style.paddingLeft);
});

test('#137: подпапки сортируются по имени при назначенной роли', async () => {
  const extra = {...yandexFolders[1].find(folder => folder.id === 20), id: 22, remote_path: 'Trash|z-last', display_name: 'z-last', role: 'archive'};
  const app = await renderMail([accounts[1]], [[...yandexFolders[1], extra]]);
  const ids = navRows(app).map(row => Number(row.dataset.folderId));
  const parentIndex = ids.indexOf(19);
  assert.deepEqual(ids.slice(parentIndex, parentIndex + 3), [19, 20, 22]);
});
