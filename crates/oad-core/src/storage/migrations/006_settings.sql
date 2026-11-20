-- 服务器自己的设置（键值）。现在只有签文件地址的密钥（url_signing_key），第一次用到时随机生成
CREATE TABLE settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
