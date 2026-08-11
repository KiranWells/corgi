use base64::{Engine, prelude::BASE64_URL_SAFE_NO_PAD};

const SAFE_CHARS: &str = "@#%*";
const TEMPLATES: [&str; 4] = [
    r"
                      .        
                     .#~~      
                 ~++~+@@+.:    
                .*@@@@@@@@@@~  
         \ +~+  @@@@@@@@@@@@@: 
        ~:*@@@@~@@@@@@@@@@@@*. 
 ~~~+~~+##@@@@@@@@@@@@@@@@@+   
        ~:*@@@@~@@@@@@@@@@@@*. 
         / +~+  @@@@@@@@@@@@@: 
                .*@@@@@@@@@@~  
                 ~++~+@@+.:    
                     .#~~      
                      .        ",
    r"
                           /          
                         <#@#>        
                    \     %@%.        
                     #@@%@@@@@#%*+.   
           \  :.   =*@@@@@@@@@@@@@%:  
            +@%@@= @@@@@@@@@@@@@@@@^  
         \|*@@@@@@+@@@@@@@@@@@@@@@+   
 ~~~~|~~~**#@@@@@@*@@@@@@@@@@@@@@<    
         /|*@@@@@@+@@@@@@@@@@@@@@@+   
            +@%@@= @@@@@@@@@@@@@@@@.  
           /  :^   =*@@@@@@@@@@@@@%:  
                     #@@%@@@@@#%*+^   
                    /     %@%^        
                         <#@#>        
                           \          ",
    r"
@@@@@@@@@@@@@@@@@@%%#* ~ #%@@@@@
@@@@@@@@@@@@@@@%%%##+.+~.*#%%%@@
@@@@@@@@@@@@%%%%*.... @+ .~~.*%@
@@@@@@@%#+*####*~~@@@@@@@@#~ ~#@
@@@@%%%%*.   .~~+@@@@@@@@@@@+ #@
%%##***+. #@@@#~@@@@@@@@@@@@#~#@
#~~~~:~*~+@@@@@@@@@@@@@@@@@+~*%@
%%##***+. #@@@#~@@@@@@@@@@@@#~#@
@@@@%%%%*.   .~~+@@@@@@@@@@@+ #@
@@@@@@@%#+*####*~~@@@@@@@@#~ ~#@
@@@@@@@@@@@@%%%%*.... @+ .~~.*%@
@@@@@@@@@@@@@@@%%%##+.+~.*#%%%@@
@@@@@@@@@@@@@@@@@@%%#* ~ #%@@@@@",
    r"
#@%%@@@#@@@@@%#%#@@@#%@%@@*@@@%#@@~%%+@@*
#*%@%@@%+@@%@@@@#@@%@#@#@%**+*%#%**=*~+=%
@@***@@%*@@#%%@@#%#@@@%@%#%+...~=::=..~%+
@@@@%@%@%##@*##@#@+*#%+%%%:~+%=~~..::.%##
@@@@@@@@@@%#:~~+**::.~~:=+~:++*+=.:~*##@%
#@@%@@%%%##=~...:.+=~=*~:..~~=#*#+~.***##
*#@@@@@%@%#%:....*#*#~.~::.::~.++*:.+~@@@
@@*@@@#@@##+.=+=**~%*~~=~..:~::=~:~:+#%%#
%*@@#@@##*+..+**#@%##=.+:::..=..~**%#%@%%
@@@@@@@#@#%+.=~:+@@@=%%*++:+:*@##@@%#%%@@
@@*%#%%@%%*%#+++##*%%=%@@::~::%#@#@%@@@@%
@@%@%%##+=~.:::.:~.:=+##%%=#~.:#@*%##%###
#@%#++~~~~*+~~~.~.:::~@*#*+*=.:*#%@*@@%#@
@@@@*..:***~:....~=:.*##%::.~~+*%@%@@@@@#
%%@%=~=~#*#:~:.~~.~+**#*..:..::#*##%@%@@@
@%%##+..=*#%*~:~=+~=~..~=+%==~:*%@@@@@@@@
##%~.:..::.+%#*.**+%*%**%*#@##@%@*%@#+*%@
%%*.~=:..:~*=~.~##@@+#@%*#%@@%%%@@*@%@%*%
**+..:~:++:~:.~#@@@%#@@%@#@@@@@@@@+@@%@@@
@@+=#==%#%%@%%##@@@%@%@@@%*%*+#%@@@@@@@%%",
];

fn create_padded_template(templates: &[&str; 4], b64_str: &str) -> String {
    let mut template = String::from(templates[templates.len() - 1]);
    while template
        .chars()
        .filter(|ch| SAFE_CHARS.contains(*ch))
        .count()
        < b64_str.len()
    {
        let width = template.lines().next().unwrap().len();
        let height = template.lines().count();
        if width as f64 > height as f64 * 2.1 {
            template = format!("{}\n{template}\n{}", "@".repeat(width), "@".repeat(width));
        } else {
            template = template.lines().map(|l| format!("@{l}@\n")).collect();
            template = template.strip_suffix('\n').unwrap().to_string();
        }
    }
    template
}

fn safe_count(template: &str) -> usize {
    template
        .chars()
        .filter(|ch| SAFE_CHARS.contains(*ch))
        .count()
}

pub fn create_share_string(bytes: &[u8]) -> String {
    let mut b64_str = String::new();
    BASE64_URL_SAFE_NO_PAD.encode_string(bytes, &mut b64_str);

    let templates = TEMPLATES.map(|s| s.strip_prefix("\n").unwrap());
    let template = templates
        .iter()
        .find(|s| safe_count(s) > b64_str.len())
        .map(|s| String::from(*s))
        .unwrap_or_else(|| create_padded_template(&templates, &b64_str));

    let padding = (safe_count(&template) - b64_str.len()) / 2;
    let mut output = String::new();
    let mut skipped = 0;
    b64_str = b64_str.chars().rev().collect();
    for ch in template.chars() {
        if SAFE_CHARS.contains(ch) {
            if skipped < padding {
                skipped += 1;
                output.push(ch);
                continue;
            }
            output.push(b64_str.pop().unwrap_or(ch));
        } else {
            output.push(ch);
        }
    }

    output
}

pub fn parse_share_string(string: &str) -> color_eyre::Result<Vec<u8>> {
    let mut b64_str = String::new();
    for ch in string.chars() {
        if base64::alphabet::URL_SAFE.as_str().contains(ch) {
            b64_str.push(ch);
        }
    }

    let mut bytes = Vec::new();
    BASE64_URL_SAFE_NO_PAD.decode_vec(b64_str, &mut bytes)?;
    Ok(bytes)
}
