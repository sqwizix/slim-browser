# Пакет Slim Browser

`make-icon.ps1` создаёт собственный значок. `build-installer.ps1` упаковывает собранный EXE через makecab и Windows Installer COM; сторонний компилятор установщиков не требуется.

Установщик проверенной версии: `C:\Users\sqwizix\Desktop\Slim Browser\Установщик\SlimBrowser-Setup.msi`. Он установлен и проверен, включая повторную установку из этого же неизменённого пакета. Храните исходный MSI: при восстановлении Windows проверяет его хеш. Для нового выпуска меняйте ProductVersion и ProductCode; не заменяйте установленный пакет другим MSI с тем же ProductCode.

MSI создаёт файлы приложения и ярлык меню «Пуск». Профиль в `%LOCALAPPDATA%/SlimBrowserRust` не входит в состав MSI и сохраняется после удаления приложения. По умолчанию устанавливается в `%LOCALAPPDATA%/Programs/Slim Browser`; при установке этой версии указан каталог на рабочем столе через INSTALLDIR.
