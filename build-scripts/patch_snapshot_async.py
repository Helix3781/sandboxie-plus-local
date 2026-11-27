import io, re, sys

path = r"D:\kaifa-soft\_build\Sandboxie\SandboxiePlus\QSbieAPI\Sandboxie\SandBox.cpp"
src = io.open(path, encoding="utf-8-sig").read()

start_marker = "SB_PROGRESS CSandBox::SelectSnapshot(const QString& ID)"
end_marker = "SB_STATUS CSandBox::SetSnapshotInfo(const QString& ID, const QString& Name, const QString& Description)"
i = src.index(start_marker)
j = src.index(end_marker)
old = src[i:j]
assert "CleanBoxFolders" in old

new = r'''// Copy a snapshot layer's folder tree into the box root, overwriting what
// lower layers already put there.  Free function so the restore can run on a
// worker thread without touching CSandBox state.
static SB_STATUS CSandBox__CopyLayerFolder(const QString& SourceDir, const QString& TargetDir)
{
	if (!QDir(SourceDir).exists())
		return SB_OK;

	QDirIterator It(SourceDir, QDir::AllEntries | QDir::NoDotAndDotDot | QDir::Hidden | QDir::System,
					QDirIterator::Subdirectories);
	while (It.hasNext())
	{
		const QString SrcPath = It.next();
		const QString RelPath = SrcPath.mid(SourceDir.length());	// starts with '\'
		const QString DstPath = TargetDir + RelPath;
		const QFileInfo SrcInfo(SrcPath);

		if (SrcInfo.isDir())
		{
			if (!QDir().mkpath(DstPath))
				return SB_ERR(SB_SnapCopyDatFail, QVariantList() << DstPath);
			continue;
		}

		QDir().mkpath(QFileInfo(DstPath).absolutePath());

		if (QFile::exists(DstPath)) {
			QFile::setPermissions(DstPath, QFile::ReadOwner | QFile::WriteOwner);
			QFile::remove(DstPath);
		}
		if (!QFile::copy(SrcPath, DstPath))
			return SB_ERR(SB_SnapCopyDatFail, QVariantList() << DstPath);
	}
	return SB_OK;
}

// Apply one layer's incremental deletions/relocations (FilePaths.dat) to the
// box root before that layer's files land on top of it.
static SB_STATUS CSandBox__ApplyLayerDeletions(const QString& BoxPath, const QString& LayerDir)
{
	// FilePaths.dat paths are box-relative ("\drive\...");
	auto LayerPath = [&BoxPath](const QString& P) -> QString {
		if (P.startsWith("\\"))
			return BoxPath + P;
		return P;
	};
	QFile DatFile(LayerDir + "\\FilePaths.dat");
	if (!DatFile.open(QFile::ReadOnly))
		return SB_OK;

	const QByteArray DatBin = DatFile.readAll();
	DatFile.close();

	const QStringList Lines = QString::fromWCharArray((const wchar_t*)DatBin.constData(),
		DatBin.size() / sizeof(wchar_t)).split("\n");

	foreach(const QString& Line, Lines)
	{
		const QStringList Data = Line.trimmed().split("|");
		if (Data.value(0).isEmpty())
			continue;
		const QString Path = LayerPath(Data.value(0));
		bool FlagsOk = false;
		const int Flags = Data.value(1).trimmed().toInt(&FlagsOk, 16);
		Q_UNUSED(FlagsOk);

		if (Flags & FILE_RELOCATION_FLAG)
		{
			const QString Relocation = Data.size() >= 3 ? LayerPath(Data.value(2)) : QString();
			if (!Relocation.isEmpty() && QFileInfo::exists(Relocation))
			{
				if (QFileInfo(Relocation).isDir())
				{
					QDir().mkpath(Path);
					(void)CSandBox__CopyLayerFolder(Relocation, Path);
					QDir(Relocation).removeRecursively();
				}
				else
				{
					QDir().mkpath(QFileInfo(Path).absolutePath());
					if (QFile::exists(Path))
						QFile::remove(Path);
					QFile::copy(Relocation, Path);
					QFile::remove(Relocation);
				}
			}
		}

		if (Flags & FILE_DELETED_FLAG)
		{
			const QFileInfo Info(Path);
			if (Info.isDir())
				QDir(Path).removeRecursively();
			else if (Info.exists())
				QFile::remove(Path);
		}
	}
	return SB_OK;
}

void CSandBox::SelectSnapshotAsync(const CSbieProgressPtr& pProgress, const QString& BoxPath, const QString& ID)
{
	SB_STATUS Status;

	QSettings ini(BoxPath + "\\Snapshots.ini", QSettings::IniFormat);

	// Build the ancestor chain of the selected snapshot, root first.  SandMan's
	// own SelectSnapshot only swaps RegHive and wipes drive/user/share, leaving
	// the actual files to be "merged" by the kernel driver as a read-only lower
	// layer.  User-mode consumers (installers running chmod/stat, tools that
	// require a real directory, ...) then hit ENOENT because those files never
	// exist on disk.  We materialize the whole layer stack instead, which is
	// what a snapshot restore is supposed to mean.
	QStringList Chain;
	if (!ID.isEmpty()) {
		QString Cur = ID;
		for (int Guard = 0; !Cur.isEmpty() && Guard < 256; Guard++) {
			if (!ini.childGroups().contains("Snapshot_" + Cur))
				break;
			Chain.prepend(Cur);
			Cur = ini.value("Snapshot_" + Cur + "/Parent").toString();
		}
	}

	foreach(const SBoxDataFile& BoxDataFile, CSandBox__BoxDataFiles)
	{
		QFile::remove(BoxPath + "\\" + BoxDataFile.Name);

		if (ID.isEmpty() || BoxDataFile.Recursive)
			continue; // this one is incremental, don't restore it

		if (!QFile::copy(BoxPath + "\\snapshot-" + ID + "\\" + BoxDataFile.Name, BoxPath + "\\" + BoxDataFile.Name)) {
			if (BoxDataFile.Required) {
				pProgress->Finish(SB_ERR(SB_SnapCopyDatFail));
				return;
			}
		}
	}

	// Wipe the box folders synchronously on this worker thread.
	foreach(const QString& BoxSubFolder, CSandBox__BoxSubFolders)
	{
		Status = CSandBox__DeleteFolder(pProgress, BoxPath + "\\" + BoxSubFolder);
		if (Status.IsError()) {
			pProgress->Finish(Status);
			return;
		}
	}

	foreach(const QString& Layer, Chain)
	{
		const QString LayerDir = BoxPath + "\\snapshot-" + Layer;

		pProgress->ShowMessage(CSandBox::tr("Restoring snapshot %1").arg(Layer));

		Status = CSandBox__ApplyLayerDeletions(BoxPath, LayerDir);
		if (Status.IsError()) {
			pProgress->Finish(Status);
			return;
		}

		foreach(const QString& BoxSubFolder, CSandBox__BoxSubFolders)
		{
			Status = CSandBox__CopyLayerFolder(LayerDir + "\\" + BoxSubFolder, BoxPath + "\\" + BoxSubFolder);
			if (Status.IsError()) {
				pProgress->Finish(Status);
				return;
			}
		}
	}

	ini.setValue("Current/Snapshot", ID);
	ini.sync();

	pProgress->Finish(SB_OK);
}

SB_PROGRESS CSandBox::SelectSnapshot(const QString& ID)
{
	QSettings ini(m_FilePath + "\\Snapshots.ini", QSettings::IniFormat);

	if (!ID.isEmpty() && !ini.childGroups().contains("Snapshot_" + ID))
		return SB_ERR(SB_SnapNotFound);

	if (m_pAPI->HasProcesses(m_Name))
		return SB_ERR(SB_SnapIsRunning);

	// Always return a real progress object: the callers only dereference the
	// value when the status is OP_ASYNC, so returning a bare SB_STATUS for the
	// asynchronous path would hand them a null progress pointer.
	CSbieProgressPtr pProgress = CSbieProgressPtr(new CSbieProgress());
	QtConcurrent::run(CSandBox::SelectSnapshotAsync, pProgress, m_FilePath, ID);
	return SB_PROGRESS(OP_ASYNC, pProgress);
}

'''
src = src[:i] + new + src[j:]

# header declaration
hpath = r"D:\kaifa-soft\_build\Sandboxie\SandboxiePlus\QSbieAPI\Sandboxie\SandBox.h"
h = io.open(hpath, encoding="utf-8-sig").read()
anchor = "	static void						CleanBoxAsync(const CSbieProgressPtr& pProgress, const QStringList& BoxFolders);"
assert anchor in h
h = h.replace(anchor, anchor + "\n	static void						SelectSnapshotAsync(const CSbieProgressPtr& pProgress, const QString& BoxPath, const QString& ID);")

io.open(path, "w", encoding="utf-8", newline="\n").write(src)
io.open(hpath, "w", encoding="utf-8", newline="\n").write(h)
print("patched ok", len(src), len(h))
