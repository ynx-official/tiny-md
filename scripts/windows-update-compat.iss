{ Older copied helpers pass Rust's canonicalized \\?\ path to /DIR. Normalize
  the wizard directory before Inno 6 validates it, so those clients can upgrade. }
<event('InitializeWizard')>
procedure NormalizeUpdateDirectory;
var
  Directory: String;
begin
  Directory := WizardForm.DirEdit.Text;
  if CompareText(Copy(Directory, 1, 8), '\\?\UNC\') = 0 then
    Directory := '\\' + Copy(Directory, 9, Length(Directory))
  else if (Copy(Directory, 1, 4) = '\\?\') and
    (Copy(Directory, 6, 2) = ':\') then
    Directory := Copy(Directory, 5, Length(Directory));
  if Directory <> WizardForm.DirEdit.Text then begin
    Log('Normalized the legacy update installation directory');
    WizardForm.DirEdit.Text := Directory;
  end;
end;
