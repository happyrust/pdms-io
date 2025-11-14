@echo off
echo Starting Meilisearch server...
echo.


REM 启动 Meilisearch 服务器
meilisearch.exe --master-key=masterKey123
@REM meilisearch.exe --http-addr 127.0.0.1:7700

pause 