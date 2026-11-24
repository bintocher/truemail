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
const navIds = app => navRows(app).map(row => Number(row.dataset.folderId));
const folderName = row => row.querySelector('.folder-name').textContent;
const rowById = (app, id) => app.query(`.folder-row[data-folder-id="${id}"]`);
const padding = (app, id) => parseInt(rowById(app, id).style.paddingLeft, 10);
const treeFolder = (id, display_name, parent_id = null, fields = {}) => ({
  id, display_name, parent_id, account_id: 1, remote_path: `id-${id}`, role: null, ...fields,
});

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

test('#137: подпапки Яндекса без роли сортируются по короткому имени', async () => {
  const extra = {...yandexFolders[1].find(folder => folder.id === 20), id: 22, remote_path: 'Trash|a-first', display_name: 'z-last'};
  const app = await renderMail([accounts[1]], [[extra, ...yandexFolders[1]]]);
  const ids = navIds(app);
  const parentIndex = ids.indexOf(19);
  assert.deepEqual(ids.slice(parentIndex, parentIndex + 3), [19, 20, 22]);
});

test('#137: в папке Exchange дети с ролью идут перед детьми без роли', async () => {
  const account = {id: 1, email: 'mail@exchange.example', provider: 'exchange'};
  const folders = [treeFolder(1, 'Work'), treeFolder(2, 'Notes', 1), treeFolder(3, 'Trash', 1, {role: 'trash'})];
  const app = await renderMail([account], [folders]);
  assert.deepEqual(navIds(app), [1, 3, 2]);
});

test('#137: цикл родителей JMAP сохраняет все папки и ветку под первой папкой цикла', async () => {
  const folders = [treeFolder(1, 'A', 2), treeFolder(2, 'B', 1), treeFolder(3, 'C', 1), treeFolder(4, 'D')];
  const app = await renderMail([accounts[0]], [folders]);
  assert.deepEqual(navIds(app), [4, 1, 2, 3]);
  assert.equal(padding(app, 1), padding(app, 4));
  assert.equal(padding(app, 3), padding(app, 2));
  assert.ok(padding(app, 3) > padding(app, 1));
});

test('#137: папка со ссылкой на себя показывается один раз на верхнем уровне', async () => {
  const folders = [treeFolder(1, 'A', 1), treeFolder(2, 'B')];
  const app = await renderMail([accounts[0]], [folders]);
  assert.deepEqual(navIds(app), [2, 1]);
  assert.equal(padding(app, 1), padding(app, 2));
});

test('#137: обычная цепочка из трёх папок сохраняет глубину', async () => {
  const folders = [treeFolder(3, 'C', 2), treeFolder(2, 'B', 1), treeFolder(1, 'A')];
  const app = await renderMail([accounts[0]], [folders]);
  assert.deepEqual(navIds(app), [1, 2, 3]);
  assert.ok(padding(app, 1) < padding(app, 2));
  assert.ok(padding(app, 2) < padding(app, 3));
});

test('#137: ссылка на родителя из другого ящика оставляет папку на верхнем уровне', async () => {
  const local = [treeFolder(1, 'A'), treeFolder(2, 'B', 3)];
  const foreign = [treeFolder(3, 'C', null, {account_id: 2})];
  const app = await renderMail(accounts, [local, foreign]);
  assert.deepEqual(navIds(app), [1, 2, 3]);
  assert.equal(padding(app, 2), padding(app, 1));
});
