/* Copyright (c) ISWORKNEXT
 *
 * Licensed under the BSD 2-Clause license, the same terms as the rest of
 * this repository.
 */

using System.Drawing;
using System.Windows.Forms;
using XenAdmin.Actions;
using XenAdmin.TabPages;
using XenAPI;

namespace XenAdmin.SettingsPanels
{
    public class VMXenSettingsEditPage : UserControl, IEditPage
    {
        private readonly VmXenSettingsEditor editor = new VmXenSettingsEditor(VmXenSettingsArea.General);
        private VM clone;

        public VMXenSettingsEditPage()
        {
            Dock = DockStyle.Fill;
            AutoScroll = true;
            editor.UseInProperties();
            Controls.Add(editor);
        }

        public sealed override string Text => "Xen settings";

        public string SubText => string.IsNullOrEmpty(editor.Summary) ? "VM platform" : editor.Summary;

        public Image Image => Images.StaticImages._002_Configure_h32bit_16;

        public void SetXenObjects(IXenObject orig, IXenObject cloneObject)
        {
            clone = cloneObject as VM;
            editor.Bind(clone);
        }

        public AsyncAction SaveSettings()
        {
            if (clone != null && HasChanged)
                editor.ApplyTo(clone);
            return null;
        }

        public bool ValidToSave
        {
            get
            {
                if (!HasChanged)
                    return true;
                string error;
                return editor.TryValidate(out error);
            }
        }

        public void ShowLocalValidationMessages()
        {
            editor.ShowValidation();
        }

        public void HideLocalValidationMessages()
        {
            editor.HideValidation();
        }

        public void Cleanup()
        {
            editor.Bind(null);
            clone = null;
        }

        public bool HasChanged => editor.HasEdits;
    }
}
