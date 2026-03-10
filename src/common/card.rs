use serde::{Serialize, Deserialize};
use rand::seq::SliceRandom; // 提供 shuffle 方法
 
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub enum Suit {
    Hearts,    // 红桃
    Diamonds,  // 方块
    Clubs,     // 梅花
    Spades,    // 黑桃
}



#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize, Serialize)]
pub enum Rank {
    
    Three=3,
    Four,
    Five,
    Six,
    Seven,
    Eight,
    Nine,
    Ten,
    Jack,    // J
    Queen,   // Q
    King,    // K
    Ace,     // A
    Two,
    Joker,
    Colorjoker,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub struct Card {
    pub suit: Option<Suit>,
    pub rank: Rank,
}

impl Suit {
    const ALL: [Suit; 4] = [Suit::Spades, Suit::Hearts, Suit::Diamonds, Suit::Clubs];
}

impl Rank {
    const ALL: [Rank; 13] = [
        Rank::Ace, Rank::Two, Rank::Three, Rank::Four, Rank::Five, Rank::Six, Rank::Seven,
        Rank::Eight, Rank::Nine, Rank::Ten, Rank::Jack, Rank::Queen, Rank::King,
    ];
}

pub fn create_pokes() -> Vec<Card> {
    let mut cards = Vec::new();
    for suit in Suit::ALL {
        for rank in Rank::ALL {
            cards.push(Card::new(Some(suit), rank));
        }
    }
    cards.push(Card::new(None, Rank::Joker));
    cards.push(Card::new(None, Rank::Colorjoker)); 
    cards
}

pub fn shuffle() -> (Vec<Card>,Vec<Card>,Vec<Card>,Vec<Card>){
    let mut lcards = create_pokes();
    lcards.shuffle(&mut rand::rng() );

    let cards = lcards[..51].to_vec();
    let underhand = lcards[51..54].to_vec();
    let mut player1_cards = Vec::new();
    let mut player2_cards = Vec::new();
    let mut player3_cards = Vec::new();
    for (index, value) in cards.iter().enumerate() {
        // println!("{}: {}", index, value);
        if index%3 ==0{
            player1_cards.push(*value);
        }
        if index%3 ==1{
            player2_cards.push(*value);
        }
        if index%3 ==2{
            player3_cards.push(*value);
        }
    }
    (player1_cards,player2_cards,player3_cards,underhand)
}

 




impl Card {
    pub fn new(suit: Option<Suit>, rank: Rank) -> Self {
        Card { suit, rank }
    }
    
    // 获取点数（用于比较大小）
    pub fn value(&self) -> u8 {
        match self.rank {
            
            Rank::Three => 3,
            Rank::Four => 4,
            Rank::Five => 5,
            Rank::Six => 6,
            Rank::Seven => 7,
            Rank::Eight => 8,
            Rank::Nine => 9,
            Rank::Ten => 10,
            Rank::Jack => 11,
            Rank::Queen => 12,
            Rank::King => 13,
            Rank::Ace => 14, // 或1，取决于游戏规则
            Rank::Two => 15,
            Rank::Joker=>16,
            Rank::Colorjoker=>17,
        }
    }
    
    // 获取显示名称
    pub fn display_name(&self) -> String {
        let rank_str = match self.rank {
            Rank::Two => "2",
            Rank::Three => "3",
            Rank::Four => "4",
            Rank::Five => "5",
            Rank::Six => "6",
            Rank::Seven => "7",
            Rank::Eight => "8",
            Rank::Nine => "9",
            Rank::Ten => "10",
            Rank::Jack => "J",
            Rank::Queen => "Q",
            Rank::King => "K",
            Rank::Ace => "A",
            Rank::Joker => "Joker",
            Rank::Colorjoker=>"Colorjoker",

        };
        
        let suit_char = match self.suit {
            Some(Suit::Hearts) => '♥',
            Some(Suit::Diamonds) => '♦',
            Some(Suit::Clubs) => '♣',
            Some(Suit::Spades) => '♠',
            _=> '🃏',
        };
        
        format!("{}{}", rank_str, suit_char)
    }
}

//shuffle洗牌，cut切牌，deal发牌，sort理牌，draw摸牌，play打出，discard弃牌
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash,)]
pub enum PlayType {
    Single,    // 单牌
    Double,  // 对子
    San, //三个
    SanDaiyi,     // 三带一
    SanDaidui,    // 三带对
    Bomb, //炸弹
    Shunzi,//顺子
    Feiji,//飞机
    Liandui,//连对
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Poker { 
    pt:PlayType,
    max:u8,
    quantity:u8,

}

 



pub fn check(verified_card:Vec<Card>)->Option<Poker>{
    match verified_card.len(){
        1=>return Some(Poker{
            pt:PlayType::Single,
            max:verified_card[0].value(),
            quantity:1,
        }),
        2=>{
            if (verified_card[0].rank == Rank::Joker && verified_card[1].rank == Rank::Colorjoker)||
            (verified_card[1].rank == Rank::Joker && verified_card[0].rank == Rank::Colorjoker){
                return Some(Poker{
                    pt:PlayType::Bomb,
                    max:17,
                    quantity:2,
                })

            }else if verified_card[0].value()==verified_card[1].value(){
                return Some(Poker{
                    pt:PlayType::Double,
                    max:verified_card[0].value(),
                    quantity:2,
                })
            }else{
                return None;
            }
        },
        3=>{
            if verified_card[0].value()==verified_card[1].value()&&verified_card[1].value()==verified_card[2].value(){
                return Some(Poker{
                    pt:PlayType::San,
                    max:verified_card[0].value(),
                    quantity:3,
                })
            }else {
                return  None;
            }
        },
        4=>{
            if verified_card[0].value()==verified_card[1].value()&&verified_card[1].value()==verified_card[2].value()&&verified_card[2].value()==verified_card[3].value(){
                return Some(Poker{
                    pt:PlayType::Bomb,
                    max:verified_card[0].value(),
                    quantity:4,
                });
            }
            let mut map = std::collections::HashMap::<u8,u8>::new();
            for tmpcard in verified_card{
                let mut count= *map.get(&tmpcard.value()).unwrap_or(&0);
                count = count+1;
                map.insert(tmpcard.value(), count);
            }
            if map.len()==2{
                let mut rank=0;
                for (key,value) in map.iter(){
                    if *value==3{
                        rank=*key;
                    }
                }
                return Some(Poker{
                    pt:PlayType::SanDaiyi,
                    max:rank,
                    quantity:3,
                })
            }

            return None;
        },
        _=>{
            let q_len= verified_card.len();
            let mut map = std::collections::HashMap::<u8,u8>::new();
            for tmpcard in verified_card{
                let mut count= *map.get(&tmpcard.value()).unwrap_or(&0);
                count = count+1;
                map.insert(tmpcard.value(), count);
            }

            

            let mut vec_one = Vec::new();
            let mut vec_two = Vec::new();
            let mut vec_three = Vec::new();

            for (key,value) in map.iter(){
                if *value==3{
                    vec_three.push(*key);
                }
                if *value==2{
                    vec_two.push(*key);
                }
                if *value==1{
                    vec_one.push(*key);
                }
            }
            if vec_two.len()==0&&vec_three.len()==0{
                let vc = map.keys().into_iter().cloned().collect::<Vec<_>>();
                let (is_consecutive,max_val) = is_consecutive_hashset(&vc);
                if is_consecutive{
                    return Some(Poker{
                        pt:PlayType::Shunzi,
                        max:max_val,
                        quantity:q_len as u8,
                    })
                }
            }


            if (vec_three.len()== vec_two.len()&& vec_one.len()==0)||
            (vec_three.len()== vec_one.len()&& vec_two.len()==0){
                let (res,max) = is_consecutive_hashset(&vec_three);
                if res {
                    return Some(Poker{
                        pt:PlayType::Feiji,
                        max:max,
                        quantity:3*vec_three.len() as u8,
                    })
                }
                 
            }
            if vec_one.len() ==0 && vec_three.len()==0{
                let (res,max) = is_consecutive_hashset(&vec_two);
                if res {
                    return Some(Poker{
                        pt:PlayType::Liandui,
                        max:max,
                        quantity:2*vec_two.len() as u8,
                    })
                }
            }
            return None;
        
        }
    }
    
}



pub fn compare(last:Vec<Card>,old:Vec<Card>)->bool{
    if old.len()==0{
        return true;
    }
    let last_poke = check(last.clone());
    let old_poke  = check(old.clone());
    match last_poke {
        Some(last_poke1)=>{
            let old_poke2 = old_poke.unwrap();
            if last_poke1.pt == PlayType::Bomb{
                if old_poke2.pt ==PlayType::Bomb&& last_poke1.max<old_poke2.max{
                    return false;
                }else{
                    return true;
                }
            }else{
                if last.len() == old.len()&& 
                last_poke1.quantity == old_poke2.quantity && 
                last_poke1.max>old_poke2.max{
                    return true;
                }
            }
        },
        None =>{
            return false;
        }
    }

    return false;

}



use std::collections::HashSet;

fn is_consecutive_hashset(nums: &[u8]) -> (bool,u8) {
    if nums.is_empty() {
        return (true,0);
    }
    
    let set: HashSet<u8> = nums.iter().copied().collect();
    
    // 如果 HashSet 大小不等于数组大小，说明有重复数字
    if set.len() != nums.len() {
        return (false,0);
    }
    
    // 找到最小值和最大值
    let min_val = *nums.iter().min().unwrap();
    let max_val = *nums.iter().max().unwrap();
    
    // 对于连续数字：最大值 - 最小值 + 1 应该等于数字个数
    ((max_val - min_val + 1) as usize == nums.len(),max_val)
}




#[cfg(test)]
mod card{
    use crate::common::card::{Card, Rank, Suit, check};
    #[test]
    fn test_card(){
        let mut a = Vec::new();
         
        // a.push(Card::new( Suit::Clubs, Rank::King) );
        // a.push(Card::new( Suit::Spades, Rank::King) );
        // a.push(Card::new( Suit::Diamonds, Rank::Queen) );
        // // a.push(Card::new( Suit::Clubs, Rank::Queen) );
        // a.push(Card::new( Suit::Clubs, Rank::Jack) );
        // a.push(Card::new( Suit::Clubs, Rank::Ten) );
        let res  = check(a);
        println!("{:?}",res.unwrap());
    }

}