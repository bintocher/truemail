; Хуки установщика truemail. Подключаются шаблоном installer.nsi через
; bundle.windows.nsis.installerHooks в tauri.conf.json.
;
; Пункт "Отправить -> truemail" в проводнике - это обычный ярлык в папке SendTo
; текущего пользователя. Программа принимает пути файлов аргументами и открывает
; новое письмо с ними во вложениях. Ту же галочку можно включить и выключить в
; настройках программы (get_sendto_shortcut / set_sendto_shortcut в commands.rs);
; имя ярлыка там задано строкой "truemail.lnk" и должно совпадать с PRODUCTNAME.

!macro NSIS_HOOK_POSTINSTALL
  ; Папка SendTo всегда пользовательская, даже при установке на всю машину.
  SetShellVarContext current
  ; При обновлении ярлык только поправляем, если он есть: создавать заново
  ; нельзя - пользователь мог убрать пункт из меню сам.
  ${If} $UpdateMode <> 1
  ${OrIf} ${FileExists} "$APPDATA\Microsoft\Windows\SendTo\${PRODUCTNAME}.lnk"
    CreateShortcut "$APPDATA\Microsoft\Windows\SendTo\${PRODUCTNAME}.lnk" \
      "$INSTDIR\${MAINBINARYNAME}.exe" "" "$INSTDIR\${MAINBINARYNAME}.exe" 0
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; При обновлении удалять нечего: следом отработает POSTINSTALL новой версии.
  ${If} $UpdateMode <> 1
    SetShellVarContext current
    Delete "$APPDATA\Microsoft\Windows\SendTo\${PRODUCTNAME}.lnk"
  ${EndIf}
!macroend
