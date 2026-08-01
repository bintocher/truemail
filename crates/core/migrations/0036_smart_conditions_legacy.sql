-- Условия умных папок из ранних версий остались в старом словаре: поля
-- назывались по-русски ("Статус") или по-старому ("from", "status"), значения
-- статуса писались как seen/not_seen, вложения - yes/no. Интерфейс приводил их
-- к нынешним именам только при показе, а выборка писем в ядре сравнивает то,
-- что лежит в базе, - поэтому такие папки (например "Непрочитанные (все)")
-- оставались пустыми, пока пользователь не пересохранит условия руками.

UPDATE smart_conditions SET field = CASE field
    WHEN 'Отправитель' THEN 'sender'
    WHEN 'Sender' THEN 'sender'
    WHEN 'from' THEN 'sender'
    WHEN 'Получатель' THEN 'recipient'
    WHEN 'Recipient' THEN 'recipient'
    WHEN 'to' THEN 'recipient'
    WHEN 'Тема' THEN 'subject'
    WHEN 'Subject' THEN 'subject'
    WHEN 'Текст письма' THEN 'body'
    WHEN 'Message text' THEN 'body'
    WHEN 'Аккаунт' THEN 'account'
    WHEN 'Account' THEN 'account'
    WHEN 'Статус' THEN 'read_state'
    WHEN 'Status' THEN 'read_state'
    WHEN 'status' THEN 'read_state'
    WHEN 'Вложение' THEN 'attachment'
    WHEN 'Attachment' THEN 'attachment'
    WHEN 'Метка' THEN 'label'
    WHEN 'Label' THEN 'label'
    WHEN 'Папка' THEN 'folder'
    WHEN 'Folder' THEN 'folder'
    WHEN 'Дата' THEN 'date'
    WHEN 'Date' THEN 'date'
    ELSE field
END;

UPDATE smart_conditions SET op = CASE op
    WHEN 'содержит' THEN 'contains'
    WHEN 'не содержит' THEN 'not_contains'
    WHEN 'does not contain' THEN 'not_contains'
    WHEN 'равно' THEN 'equals'
    WHEN 'не равно' THEN 'not_equals'
    ELSE op
END;

UPDATE smart_conditions SET value = CASE value
    WHEN 'seen' THEN 'read'
    WHEN 'not_seen' THEN 'unread'
    WHEN 'Прочитано' THEN 'read'
    WHEN 'Непрочитано' THEN 'unread'
    WHEN 'Не прочитано' THEN 'unread'
    WHEN 'Read' THEN 'read'
    WHEN 'Unread' THEN 'unread'
    ELSE value
END
WHERE field = 'read_state';

UPDATE smart_conditions SET value = CASE value
    WHEN 'yes' THEN 'has'
    WHEN 'no' THEN 'none'
    WHEN 'Есть' THEN 'has'
    WHEN 'Нет' THEN 'none'
    ELSE value
END
WHERE field = 'attachment';
