; Хуки установщика truemail. Подключаются шаблоном installer.nsi через
; bundle.windows.nsis.installerHooks в tauri.conf.json.
;
; Пункт "Отправить -> truemail" в проводнике - это обычный ярлык в папке SendTo
; текущего пользователя. Создаёт его сама программа при первом запуске
; (ensure_sendto_shortcut в main.rs): установщик при обновлении отрабатывает с
; /UPDATE, и создание здесь не досталось бы тем, кто уже пользуется программой.
; Установщику остаётся поправить цель уже существующего ярлыка после переезда
; каталога установки и убрать пункт при удалении программы.

!macro NSIS_HOOK_POSTINSTALL
  ; Папка SendTo всегда пользовательская, даже при установке на всю машину.
  SetShellVarContext current
  ${If} ${FileExists} "$APPDATA\Microsoft\Windows\SendTo\${PRODUCTNAME}.lnk"
    CreateShortcut "$APPDATA\Microsoft\Windows\SendTo\${PRODUCTNAME}.lnk" \
      "$INSTDIR\${MAINBINARYNAME}.exe" "" "$INSTDIR\${MAINBINARYNAME}.exe" 0
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; При обновлении пункт не трогаем: программа новой версии продолжит им
  ; пользоваться, а полное удаление должно убрать его из меню.
  ${If} $UpdateMode <> 1
    SetShellVarContext current
    Delete "$APPDATA\Microsoft\Windows\SendTo\${PRODUCTNAME}.lnk"
    ; Метка "пункт уже заводили" (ensure_sendto_shortcut в main.rs): без неё
    ; переустановленная программа не завела бы пункт заново.
    Delete "$LOCALAPPDATA\${PRODUCTNAME}\sendto-initialized"
  ${EndIf}
!macroend
