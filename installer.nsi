Unicode True
Name "MCDB 1.2"
OutFile "dist\MCDB-1.2-release-opensource.exe"
InstallDir "$PROGRAMFILES64\MCDB"
InstallDirRegKey HKLM "Software\MCDB" "InstallDir"
RequestExecutionLevel admin
Icon "assets\icon.ico"
UninstallIcon "assets\icon.ico"
SetCompressor /SOLID lzma

Page directory
Page instfiles
UninstPage uninstConfirm
UninstPage instfiles

Section "Install"
  SetOutPath "$INSTDIR"
  File "target\release\ModDB.exe"
  File "assets\icon.ico"
  CreateDirectory "$SMPROGRAMS\MCDB"
  CreateShortCut "$SMPROGRAMS\MCDB\MCDB.lnk" "$INSTDIR\ModDB.exe" "" "$INSTDIR\ModDB.exe"
  CreateShortCut "$DESKTOP\MCDB.lnk" "$INSTDIR\ModDB.exe" "" "$INSTDIR\ModDB.exe"
  WriteRegStr HKLM "Software\MCDB" "InstallDir" "$INSTDIR"
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\MCDB" "DisplayName" "MCDB 1.2"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\MCDB" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\MCDB" "DisplayIcon" "$INSTDIR\ModDB.exe"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\MCDB" "Publisher" "NeofastFTL"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\MCDB" "DisplayVersion" "1.2.0"
  WriteRegDWORD HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\MCDB" "NoModify" 1
  WriteRegDWORD HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\MCDB" "NoRepair" 1
SectionEnd

Function .onInstSuccess
  Exec "$INSTDIR\ModDB.exe"
FunctionEnd

Section "Uninstall"
  Delete "$INSTDIR\ModDB.exe"
  Delete "$INSTDIR\icon.ico"
  Delete "$INSTDIR\Uninstall.exe"
  Delete "$SMPROGRAMS\MCDB\MCDB.lnk"
  RMDir "$SMPROGRAMS\MCDB"
  Delete "$DESKTOP\MCDB.lnk"
  RMDir "$INSTDIR"
  DeleteRegKey HKLM "Software\MCDB"
  DeleteRegKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\MCDB"
SectionEnd
