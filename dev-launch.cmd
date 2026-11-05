@echo off
cd /d D:\github\xiaolu
echo ==== START %DATE% %TIME% ==== > dev-launch.log
call npm run tauri:dev >> dev-launch.log 2>&1
echo ==== EXIT %ERRORLEVEL% ==== >> dev-launch.log
