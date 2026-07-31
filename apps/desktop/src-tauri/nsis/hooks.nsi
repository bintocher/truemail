; Хуки установщика truemail. Подключаются шаблоном installer.nsi через
; bundle.windows.nsis.installerHooks в tauri.conf.json.
;
; Пункт "Отправить -> truemail" в проводнике - это обычный ярлык в папке SendTo
; текущего пользователя. Программа принимает пути файлов аргументами и открывает
; новое письмо с ними во вложениях. Ту же галочку можно включить и выключить в
; настройках программы (get_sendto_shortcut / set_sendto_shortcut в commands.rs):
; там имя ярлыка берётся из productName, то есть из того же ${PRODUCTNAME}.

!macro NSIS_HOOK_POSTINSTALL
  ; Папка SendTo всегда пользовательская, даже при установке на всю машину.
  SetShellVarContext current
  ${If} ${FileExists} "$APPDATA\Microsoft\Windows\SendTo\${PRODUCTNAME}.lnk"
    ; Ярлык уже стоит - обновляем цель на новый путь установки.
    CreateShortcut "$APPDATA\Microsoft\Windows\SendTo\${PRODUCTNAME}.lnk" \
      "$INSTDIR\${MAINBINARYNAME}.exe" "" "$INSTDIR\${MAINBINARYNAME}.exe" 0
  ${ElseIf} $UpdateMode <> 1
  ${AndIf} $NoShortcutMode <> 1
    ; Создаём только на обычной установке: при обновлении пункт мог быть убран
    ; пользователем, а установка с /NS ярлыков не создаёт вовсе.
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
