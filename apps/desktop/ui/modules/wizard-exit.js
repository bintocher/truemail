// truemail UI module: wizard-exit.js
// Чистые функции без DOM и Tauri API: доступность выхода из повторного
// мастера первичной настройки и порядок обработки клавиши Escape единым
// обработчиком. Подключается в index.html перед calendar-contacts.js и
// i18n-onboarding.js как обычный скрипт и отдаёт функции через глобальный
// объект wizardExit. См. specs/setup-wizard-exit.md.

// S-001, S-002, S-007: выход разрешён только по сохранённому в ядре признаку
// настройки onboarding_completed, а не по изменяемому значению JavaScript в
// текущем окне - оно может быть устаревшим (например, до ответа позднего
// подключения аккаунта) или не отражать то, что действительно записано.
// Нечитаемое значение (ошибка чтения, undefined) считается незавершённой
// настройкой - действие выхода не показывается.
function wizardExitAllowed(savedOnboardingCompleted){
  return savedOnboardingCompleted==='true';
}

// S-006, S-011: единый обработчик Escape закрывает ровно один - самый верхний -
// открытый элемент за одно нажатие: сначала вспомогательные меню, затем
// модальные окна, и только когда над мастером ничего не осталось - сам мастер
// (S-004, S-005). Возвращает 'popup', 'overlay', 'wizard' или 'none'.
function wizardEscapeAction(state){
  const {popupMenuOpen=false,overlayOpen=false,wizardOpen=false,wizardExitAvailable=false,editingField=false}=state||{};
  if(popupMenuOpen)return 'popup';
  if(overlayOpen)return 'overlay';
  // Escape в поле ввода мастера привычно отменяет сам ввод: закрывать по нему
  // весь мастер значило бы терять введённый адрес или код подтверждения от
  // случайного нажатия (S-013).
  if(wizardOpen&&editingField)return 'none';
  if(wizardOpen&&wizardExitAvailable)return 'wizard';
  return 'none';
}

const wizardExit={wizardExitAllowed,wizardEscapeAction};
if(typeof module!=='undefined'&&module.exports)module.exports=wizardExit;
